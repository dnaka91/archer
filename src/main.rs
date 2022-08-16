use anyhow::Result;
use tokio::task::JoinHandle;
use tokio_shutdown::Shutdown;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{filter::Targets, prelude::*};

mod agent;
mod collector;
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

    tokio::try_join!(
        flatten(tokio::spawn(agent::run(shutdown.clone()))),
        flatten(tokio::spawn(collector::run(shutdown.clone()))),
        flatten(tokio::spawn(query::run(shutdown))),
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
