#![deny(rust_2018_idioms, clippy::all, clippy::pedantic)]
#![warn(clippy::expect_used, clippy::unwrap_used)]
#![allow(clippy::needless_pass_by_value)]

use anyhow::Result;
use opentelemetry::sdk::{trace, Resource};
use opentelemetry_semantic_conventions::resource;
use tokio::task::JoinHandle;
use tokio_shutdown::Shutdown;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{filter::Targets, prelude::*};

mod convert;
mod jaeger;
mod models;
mod net;
mod otel;
mod quiver;
mod storage;
mod tracer;

#[tokio::main]
async fn main() -> Result<()> {
    let database = storage::init().await?;
    let database_ro = storage::init_readonly().await?;
    let shutdown = Shutdown::new()?;

    let tracer = tracer::install_batch(
        database.clone(),
        trace::config().with_resource(Resource::new([
            resource::SERVICE_NAME.string(env!("CARGO_PKG_NAME")),
            resource::SERVICE_VERSION.string(env!("CARGO_PKG_VERSION")),
        ])),
    );

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                Targets::new()
                    .with_default(LevelFilter::WARN)
                    .with_target(env!("CARGO_CRATE_NAME"), LevelFilter::DEBUG)
                    .with_target("tower_http", LevelFilter::DEBUG),
            ),
        )
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_filter(
                    Targets::new()
                        .with_default(LevelFilter::OFF)
                        .with_target("archer::jaeger::query", LevelFilter::INFO),
                ),
        )
        .init();

    tokio::try_join!(
        flatten(tokio::spawn(jaeger::agent::run(
            shutdown.clone(),
            database.clone()
        ))),
        flatten(tokio::spawn(jaeger::collector::run(
            shutdown.clone(),
            database.clone()
        ))),
        flatten(tokio::spawn(jaeger::query::run(
            shutdown.clone(),
            database_ro
        ))),
        flatten(tokio::spawn(otel::collector::run(
            shutdown.clone(),
            database.clone(),
        ))),
        flatten(tokio::spawn(quiver::collector::run(shutdown, database))),
    )?;

    Ok(())
}

async fn flatten<T>(handle: JoinHandle<Result<T>>) -> Result<T> {
    match handle.await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(err.into()),
    }
}
