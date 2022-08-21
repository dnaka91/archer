use anyhow::Result;
use tokio::task::JoinHandle;
use tokio_shutdown::Shutdown;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{filter::Targets, prelude::*};

mod convert;
mod jaeger;
mod models;
mod otel;
mod storage;

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

    let database = storage::init().await?;
    let shutdown = Shutdown::new()?;

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
            database.clone()
        ))),
        flatten(tokio::spawn(otel::collector::run(shutdown, database))),
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
