use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Result;
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, get_service},
    Json, Router, Server,
};
use serde::Serialize;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{filter::Targets, prelude::*};

mod models;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            Targets::new()
                .with_default(LevelFilter::WARN)
                .with_target(env!("CARGO_CRATE_NAME"), LevelFilter::TRACE)
                .with_target("tower_http", LevelFilter::DEBUG),
        )
        .init();

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
        .fallback(
            get_service(
                ServeDir::new("jaeger-ui/packages/jaeger-ui/build").fallback(ServeFile::new(
                    "jaeger-ui/packages/jaeger-ui/build/index.html",
                )),
            )
            .handle_error(handle_error),
        )
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 8080));
    println!("listening on http://{addr}");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

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

async fn handle_error(_: std::io::Error) -> impl IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "failed serving static asset",
    )
}
