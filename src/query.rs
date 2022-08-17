use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    time::UNIX_EPOCH,
};

use anyhow::Result;
use archer_proto::jaeger::api_v2::SpanRefType;
use axum::{
    extract::{Path, Query},
    headers::IfNoneMatch,
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, LAST_MODIFIED},
        HeaderMap, HeaderValue, StatusCode, Uri,
    },
    response::IntoResponse,
    routing::get,
    Extension, Json, Router, Server, TypedHeader,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tokio_shutdown::Shutdown;
use tower::ServiceBuilder;
use tower_http::ServiceBuilderExt;
use tracing::{info, instrument};

use crate::{models::http, storage::Database};

#[instrument(name = "query", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let app = Router::new()
        .route("/api/services", get(services))
        .route("/api/services/:service/operations", get(operations))
        .route("/api/operations", get(todo))
        .route("/api/traces", get(traces))
        .route("/api/traces/:id", get(todo))
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

#[allow(dead_code)]
enum ApiResponse<T> {
    Data(Vec<T>),
    Errors(Vec<ApiError>),
}

impl<T> Serialize for ApiResponse<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Response<'a, T> {
            data: &'a [T],
            total: usize,
            limit: usize,
            offset: usize,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            errors: &'a [ApiError],
        }

        let resp = match self {
            Self::Data(data) => Response {
                data,
                total: data.len(),
                limit: 0,
                offset: 0,
                errors: &[],
            },
            Self::Errors(errors) => Response {
                data: &[],
                total: 0,
                limit: 0,
                offset: 0,
                errors,
            },
        };

        resp.serialize(serializer)
    }
}

#[derive(Serialize)]
struct ApiError {
    code: u32,
    msg: String,
    #[serde(rename = "traceID")]
    trace_id: TraceId,
}

#[derive(Serialize)]
#[serde(transparent)]
struct TraceId(String);

async fn services(Extension(db): Extension<Database>) -> impl IntoResponse {
    Json(ApiResponse::Data(db.list_services().await.unwrap()))
}

async fn operations(
    Path(service): Path<String>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    Json(ApiResponse::Data(
        db.list_operations(service).await.unwrap(),
    ))
}

#[derive(Deserialize)]
struct TracesParams {
    service: String,
    operation: Option<String>,
}

async fn traces(
    Query(params): Query<TracesParams>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    let spans = db
        .list_spans(params.service.clone(), params.operation)
        .await
        .unwrap();

    let traces = spans
        .into_iter()
        .group_by(|span| span.trace_id.clone())
        .into_iter()
        .map(|(trace_id, spans)| {
            let mut spans = spans.peekable();
            http::Trace {
                trace_id: http::TraceId(
                    spans
                        .peek()
                        .map(|s| hex::encode(&s.trace_id))
                        .unwrap_or_default(),
                ),
                spans: spans
                    .into_iter()
                    .map(|span| http::Span {
                        trace_id: http::TraceId(hex::encode(span.trace_id)),
                        span_id: http::SpanId(hex::encode(span.span_id)),
                        parent_span_id: None,
                        flags: span.flags,
                        operation_name: span.operation_name,
                        references: span
                            .references
                            .into_iter()
                            .map(|r| http::Reference {
                                ref_type: match r.ref_type() {
                                    SpanRefType::ChildOf => http::ReferenceType::ChildOf,
                                    SpanRefType::FollowsFrom => http::ReferenceType::FollowsFrom,
                                },
                                trace_id: http::TraceId(hex::encode(r.trace_id)),
                                span_id: http::SpanId(hex::encode(r.span_id)),
                            })
                            .collect(),
                        start_time: std::time::SystemTime::try_from(span.start_time.unwrap())
                            .unwrap()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_micros() as u64,
                        duration: std::time::Duration::try_from(span.duration.unwrap())
                            .unwrap()
                            .as_micros() as u64,
                        tags: vec![],
                        logs: vec![],
                        process_id: http::ProcessId("test".to_owned()),
                        process: None,
                        warnings: vec![],
                    })
                    .collect(),
                processes: [(
                    "test".to_owned(),
                    http::Process {
                        service_name: params.service.clone(),
                        tags: vec![],
                    },
                )]
                .into_iter()
                .collect(),
                warnings: vec![],
            }
        })
        .collect::<Vec<_>>();

    Json(ApiResponse::Data(traces))
}

async fn dependencies() -> impl IntoResponse {
    Json(ApiResponse::Data(Vec::<()>::new()))
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
