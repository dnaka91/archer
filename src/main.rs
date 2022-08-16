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
use tokio_shutdown::Shutdown;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{filter::Targets, prelude::*};

mod models;
mod query;

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

    let shutdown = Shutdown::new()?;
    let (query_result,) = tokio::join!(query::run(shutdown));

    query_result?;

    Ok(())
}
