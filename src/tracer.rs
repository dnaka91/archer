use std::time::Duration;

use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tokio_shutdown::Shutdown;
use tracing_archer::QuiverLayer;
use unidirs::{Directories, UnifiedDirs};

pub async fn init<S>(shutdown: Shutdown) -> Result<(QuiverLayer<S>, JoinHandle<Result<()>>)> {
    let cert_path = UnifiedDirs::simple("rocks", "dnaka91", env!("CARGO_PKG_NAME"))
        .default()
        .context("failed finding project directories")?
        .data_dir()
        .join("quiver/cert.pem");

    let cert = tokio::fs::read_to_string(cert_path).await?;

    let (layer, handle) = tracing_archer::builder()
        .with_resource(env!("CARGO_CRATE_NAME"), env!("CARGO_PKG_VERSION"))
        .with_server_cert(cert)
        .build()
        .await?;

    let handle = tokio::spawn(async move {
        shutdown.handle().await;
        handle.shutdown(Duration::from_secs(2)).await;
        Ok(())
    });

    Ok((layer, handle))
}
