use std::{
    borrow::Cow,
    fmt,
    net::{Ipv4Addr, SocketAddr},
    num::NonZeroU128,
};

use anyhow::Result;
use archer_http::{
    axum::{
        extract::{Path, Query},
        headers::IfNoneMatch,
        http::{
            header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, LAST_MODIFIED},
            HeaderMap, HeaderValue, StatusCode, Uri,
        },
        response::IntoResponse,
        routing::get,
        Extension, Router, Server, TypedHeader,
    },
    tower::ServiceBuilder,
    tower_http::ServiceBuilderExt,
    ApiError, ApiResponse, TraceId,
};
use itertools::Itertools;
use serde::Deserialize;
use tokio_shutdown::Shutdown;
use tracing::{info, instrument};

use crate::{convert, storage::Database};

#[instrument(name = "query", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let app = Router::new()
        .route("/api/services", get(services))
        .route("/api/services/:service/operations", get(operations))
        .route("/api/operations", get(todo))
        .route("/api/traces", get(traces))
        .route("/api/traces/:id", get(trace))
        .route("/api/archive/:id", get(todo))
        .route("/api/dependencies", get(dependencies))
        .route("/api/metrics/latencies", get(todo))
        .route("/api/metrics/calls", get(todo))
        .route("/api/metrics/errors", get(todo))
        .route("/api/metrics/minstep", get(todo))
        .fallback(get(asset))
        .layer(
            ServiceBuilder::new()
                .trace_for_http()
                .layer(Extension(database)),
        );

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 16686));
    info!("listening on http://{addr}");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown.handle())
        .await?;

    info!("server stopped");

    Ok(())
}

async fn services(Extension(db): Extension<Database>) -> impl IntoResponse {
    ApiResponse::Data(db.list_services().await.unwrap())
}

async fn operations(
    Path(service): Path<String>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    ApiResponse::Data(db.list_operations(service).await.unwrap())
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Deserialize)]
#[serde(untagged)]
enum TracesParams {
    TraceIds(TraceIdsQuery),
    Query(TracesQuery),
}

#[cfg_attr(test, derive(Debug, PartialEq))]
struct TraceIdsQuery(Vec<TraceId>);

impl<'de> Deserialize<'de> for TraceIdsQuery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TraceIdsQuery;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("trace IDs as map with all keys having the same value")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut ids = Vec::new();

                while let Some((k, v)) = access.next_entry::<Cow<'_, str>, Cow<'_, str>>()? {
                    if k != "traceID" {
                        return Err(serde::de::Error::custom("unknown key"));
                    }

                    ids.push(v.parse().map_err(serde::de::Error::custom)?);
                }

                Ok(TraceIdsQuery(ids))
            }
        }

        deserializer.deserialize_map(Visitor)
    }
}

#[cfg_attr(test, derive(Debug, PartialEq))]
#[derive(Deserialize)]
struct TracesQuery {
    service: String,
    operation: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deser_trace_ids() {
        let expect = TracesParams::TraceIds(TraceIdsQuery(vec![TraceId(5), TraceId(6)]));
        let result = serde_urlencoded::from_str(&format!("traceID={:032x}&traceID={:032x}", 5, 6));

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query() {
        let expect = TracesParams::Query(TracesQuery {
            service: "test".to_owned(),
            operation: None,
        });
        let result = serde_urlencoded::from_str("service=test");

        assert_eq!(expect, result.unwrap());
    }
}

async fn traces(
    Query(params): Query<TracesParams>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    let spans = match params {
        TracesParams::TraceIds(ids) => {
            let spans = db
                .find_traces(
                    ids.0
                        .iter()
                        .map(|id| NonZeroU128::new(id.0).unwrap().into()),
                )
                .await
                .unwrap();

            if spans.is_empty() {
                return ApiResponse::Error(ApiError {
                    code: StatusCode::NOT_FOUND,
                    msg: "trace id not found".into(),
                    trace_id: ids.0.get(0).copied(),
                });
            }

            spans
        }
        TracesParams::Query(query) => db.list_spans(query.service, query.operation).await.unwrap(),
    };

    let traces = spans
        .into_iter()
        .group_by(|span| span.trace_id)
        .into_iter()
        .map(|(trace_id, spans)| convert::trace_to_json(trace_id, spans))
        .collect::<Vec<_>>();

    ApiResponse::Data(traces)
}

async fn trace(
    Path(trace_id): Path<TraceId>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    let spans = db
        .find_trace(NonZeroU128::new(trace_id.0).unwrap().into())
        .await
        .unwrap();
    let trace = match spans.get(0) {
        Some(span) => convert::trace_to_json(span.trace_id, spans),
        None => {
            return ApiResponse::Error(ApiError {
                code: StatusCode::NOT_FOUND,
                msg: "trace ID not found".into(),
                trace_id: None,
            })
        }
    };

    ApiResponse::Data(vec![trace])
}

async fn dependencies() -> impl IntoResponse {
    ApiResponse::Data(Vec::<()>::new())
}

async fn todo() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

async fn asset(uri: Uri, if_none_match: Option<TypedHeader<IfNoneMatch>>) -> impl IntoResponse {
    let asset = ASSETS
        .get(uri.path())
        .or_else(|| ASSETS.get("/index.html"))
        .ok_or_else(|| (HeaderMap::new(), StatusCode::NOT_FOUND))?;

    let headers = [
        (CONTENT_TYPE, asset.mime),
        (ETAG, asset.etag),
        (LAST_MODIFIED, "Thu, 01 Jan 1970 00:00:00 GMT"),
        (CACHE_CONTROL, "public, max-age=2592000, must-revalidate"),
    ]
    .into_iter()
    .map(|(name, value)| (name, HeaderValue::from_static(value)))
    .collect::<HeaderMap>();

    let unmatched = if_none_match
        .map(|v| v.precondition_passes(&asset.etag.parse().unwrap()))
        .unwrap_or(true);

    if unmatched {
        Ok((headers, asset.content))
    } else {
        Err((headers, StatusCode::NOT_MODIFIED))
    }
}
