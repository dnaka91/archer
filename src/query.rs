use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Result;
use axum::{
    extract::{OriginalUri, Path},
    headers::IfNoneMatch,
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, LAST_MODIFIED},
        HeaderMap, HeaderValue, StatusCode, Uri,
    },
    response::IntoResponse,
    routing::get,
    Extension, Json, Router, Server, TypedHeader,
};
use serde::Serialize;
use tokio_shutdown::Shutdown;
use tower::ServiceBuilder;
use tower_http::ServiceBuilderExt;
use tracing::{info, instrument};

use crate::storage::Database;

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

async fn services() -> impl IntoResponse {
    Json(ApiResponse::Data(vec!["service-1", "service-2"]))
}

async fn operations(Path(_service): Path<String>) -> impl IntoResponse {
    Json(ApiResponse::Data(vec!["operation-1", "operation-2"]))
}

async fn traces() -> impl IntoResponse {
    Json(ApiResponse::Data(Vec::<()>::new()))
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
