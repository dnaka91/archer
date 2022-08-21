use std::net::{Ipv4Addr, SocketAddr};

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
        .sorted_by_key(|span| span.trace_id)
        .group_by(|span| span.trace_id)
        .into_iter()
        .map(|(trace_id, spans)| convert::trace_to_json(trace_id, spans))
        .collect::<Vec<_>>();

    ApiResponse::Data(traces)
}

async fn trace(
    Path(TraceId(trace_id)): Path<TraceId>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    let spans = db.find_trace(trace_id).await.unwrap();
    if spans.is_empty() {
        return ApiResponse::Error(ApiError {
            code: StatusCode::NOT_FOUND,
            msg: "trace ID not found".into(),
            trace_id: None,
        });
    }

    let trace = convert::trace_to_json(trace_id, spans);

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
