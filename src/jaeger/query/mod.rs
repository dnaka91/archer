#![allow(clippy::unused_async)]

use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
};

use anyhow::{ensure, Result};
use archer_http::{
    axum::{
        extract::{rejection::QueryRejection, Path, Query},
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
use serde::Deserialize;
use time::{Duration, OffsetDateTime};
use tokio_shutdown::Shutdown;
use tracing::{error, info, instrument};

use crate::{
    convert,
    storage::{Database, ListSpansParams},
};

mod de;

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
                .compression()
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

#[instrument(skip_all)]
async fn services(Extension(db): Extension<Database>) -> Result<impl IntoResponse, ApiError> {
    db.list_services()
        .await
        .map(ApiResponse::Data)
        .map_err(ApiError::from)
}

#[instrument(skip_all)]
async fn operations(
    Path(service): Path<String>,
    Extension(db): Extension<Database>,
) -> Result<impl IntoResponse, ApiError> {
    db.list_operations(service)
        .await
        .map(ApiResponse::Data)
        .map_err(ApiError::from)
}

#[cfg_attr(test, derive(Default, PartialEq))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TracesQuery {
    service: String,
    #[serde(default)]
    operation: String,
    #[serde(default, deserialize_with = "de::duration_micros")]
    start: Option<Duration>,
    #[serde(default, deserialize_with = "de::duration_micros")]
    end: Option<Duration>,
    #[serde(default, deserialize_with = "de::duration_human")]
    min_duration: Option<Duration>,
    #[serde(default, deserialize_with = "de::duration_human")]
    max_duration: Option<Duration>,
    #[serde(default, deserialize_with = "de::limit")]
    limit: Option<u32>,
    #[serde(default, flatten, deserialize_with = "de::tags")]
    tags: HashMap<String, String>,
}

impl TracesQuery {
    fn into_db(self) -> Result<ListSpansParams> {
        ensure!(!self.service.is_empty(), "service name must be specified");

        let now = OffsetDateTime::now_utc();
        let start = self.start.map_or(now - Duration::hours(48), |start| {
            OffsetDateTime::UNIX_EPOCH + start
        });
        let end = self.end.map_or(now, |end| OffsetDateTime::UNIX_EPOCH + end);

        ensure!(start < end, "start must be before end");

        if let (Some(min), Some(max)) = (self.min_duration, self.max_duration) {
            ensure!(min < max, " minimum duration must be smaller than maximum");
        }

        Ok(ListSpansParams {
            service: self.service,
            operation: (!self.operation.is_empty()).then_some(self.operation),
            start,
            end,
            duration_min: self.min_duration,
            duration_max: self.max_duration,
            limit: self.limit.unwrap_or(20) as _,
            tags: self.tags,
        })
    }
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct TraceIdsQuery(#[serde(deserialize_with = "de::trace_ids")] Vec<TraceId>);

#[instrument(skip_all)]
async fn traces(
    query: Result<Query<TracesQuery>, QueryRejection>,
    trace_ids: Option<Query<TraceIdsQuery>>,
    Extension(db): Extension<Database>,
) -> Result<impl IntoResponse, ApiError> {
    let spans = match (query, trace_ids) {
        (Ok(Query(query)), None) => {
            let params = query.into_db().map_err(|e| ApiError {
                code: StatusCode::BAD_REQUEST,
                msg: e.to_string().into(),
                trace_id: None,
            })?;
            db.list_spans(params).await.map_err(ApiError::from)?
        }
        (Err(_), Some(Query(ids))) => {
            let spans = db
                .find_traces(ids.0.iter().map(|id| id.0.into()))
                .await
                .map_err(ApiError::from)?;

            if spans.is_empty() {
                return Err(ApiError {
                    code: StatusCode::NOT_FOUND,
                    msg: "trace id not found".into(),
                    trace_id: ids.0.get(0).copied(),
                });
            }

            spans
        }
        (Ok(_), Some(_)) => {
            return Err(ApiError {
                code: StatusCode::BAD_REQUEST,
                msg: "can't search by trace IDs and query at the same time".into(),
                trace_id: None,
            });
        }
        (Err(e), None) => {
            return Err(ApiError {
                code: StatusCode::BAD_REQUEST,
                msg: e.to_string().into(),
                trace_id: None,
            });
        }
    };

    let traces = spans
        .into_iter()
        .map(|(trace_id, spans)| convert::trace_to_json(trace_id, spans))
        .collect();

    Ok(ApiResponse::Data(traces))
}

#[instrument(skip_all)]
async fn trace(
    Path(trace_id): Path<TraceId>,
    Extension(db): Extension<Database>,
) -> Result<impl IntoResponse, ApiError> {
    let spans = db
        .find_trace(trace_id.0.into())
        .await
        .map_err(ApiError::from)?;
    let trace_id = spans
        .first()
        .map(|span| span.trace_id)
        .ok_or_else(|| ApiError {
            code: StatusCode::NOT_FOUND,
            msg: "trace ID not found".into(),
            trace_id: None,
        })?;

    Ok(ApiResponse::Data(vec![convert::trace_to_json(
        trace_id, spans,
    )]))
}

#[instrument(skip_all)]
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

    let unmatched = if_none_match.map_or(Ok(true), |v| {
        asset.etag.parse().map(|etag| v.precondition_passes(&etag))
    });

    match unmatched {
        Ok(true) => Ok((headers, asset.content)),
        Ok(false) => Err((headers, StatusCode::NOT_MODIFIED)),
        Err(e) => {
            error!(error = ?e, "failed parsing etag");
            Err((headers, StatusCode::INTERNAL_SERVER_ERROR))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::num::NonZeroU128;

    use super::*;

    #[tokio::test]
    async fn deser_trace_ids() {
        let expect = TraceIdsQuery(vec![
            TraceId(NonZeroU128::new(5).unwrap()),
            TraceId(NonZeroU128::new(6).unwrap()),
        ]);
        let result = serde_urlencoded::from_str(&format!("traceID={:032x}&traceID={:032x}", 5, 6));

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_basic() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str("service=test");

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_tags_tuple() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            tags: [("a".to_owned(), "1".to_owned())].into_iter().collect(),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str("service=test&tag=a:1");

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_tags_json() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            tags: [("a".to_owned(), "1".to_owned())].into_iter().collect(),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str(r#"service=test&tags={"a":"1"}"#);

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_limit() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            limit: Some(5),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str("service=test&limit=5");

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_durations() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            min_duration: Some(Duration::milliseconds(10)),
            max_duration: Some(
                Duration::hours(1)
                    + Duration::minutes(12)
                    + Duration::minutes(30)
                    + Duration::seconds(45)
                    + Duration::milliseconds(120)
                    + Duration::microseconds(200),
            ),
            ..TracesQuery::default()
        };
        let result = serde_urlencoded::from_str(
            "\
            service=test&\
            minDuration=10ms&\
            maxDuration=1.2h30m45s120.2ms\
            ",
        );

        assert_eq!(expect, result.unwrap());
    }

    #[test]
    fn deser_query_all() {
        let expect = TracesQuery {
            service: "test".to_owned(),
            operation: "op1".to_owned(),
            start: Some(Duration::microseconds(1_661_232_631_416_000)),
            end: Some(Duration::microseconds(1_661_236_231_416_000)),
            limit: Some(20),
            tags: [
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "2".to_owned()),
            ]
            .into_iter()
            .collect(),
            ..TracesQuery::default()
        };
        // end=1661236231416000&limit=20&lookback=1h&maxDuration&minDuration&service=twitchid&start=1661232631416000
        let result = serde_urlencoded::from_str(
            "\
            service=test&\
            operation=op1&\
            start=1661232631416000&\
            end=1661236231416000&\
            minDuration&\
            maxDuration&\
            lookback=1h&\
            tag=a:1&\
            limit=20&\
            tags={\"b\":\"2\"}\
            ",
        );

        assert_eq!(expect, result.unwrap());
    }
}
