use std::time::Duration;

use anyhow::Result;
use tracing::{debug_span, info, instrument, warn, Level};
use tracing_subscriber::{filter::Targets, prelude::*};

#[tokio::main]
async fn main() -> Result<()> {
    let certificate = concat!(env!("CARGO_MANIFEST_DIR"), "/../.local/data/quiver/cert.pem");
    let certificate = tokio::fs::read_to_string(certificate).await?;

    let (quiver, handle) = tracing_quiver::builder()
        .with_server_cert(certificate)
        .with_resource(env!("CARGO_CRATE_NAME"), env!("CARGO_PKG_VERSION"))
        .build()
        .await?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(quiver)
        .with(
            Targets::new()
                .with_default(Level::WARN)
                .with_target(env!("CARGO_CRATE_NAME"), Level::TRACE)
                .with_target("tracing_quiver", Level::TRACE),
        )
        .init();

    greet();

    tokio::time::sleep(Duration::from_millis(100)).await;
    handle.shutdown(Duration::from_secs(1)).await;

    Ok(())
}

#[instrument(fields(awesome = true, speed = 15000))]
fn greet() {
    info!("greeting");
    println!("hello");

    debug_span!("help").in_scope(|| {
        warn!("no help available");
    });
}
