use std::time::Duration;

use anyhow::Result;
use tracing::{debug_span, info, instrument, warn};
use tracing_subscriber::prelude::*;

const CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----
MIIBWzCCAQCgAwIBAgIIOuisjYJ/sCwwCgYIKoZIzj0EAwIwITEfMB0GA1UEAwwW
cmNnZW4gc2VsZiBzaWduZWQgY2VydDAgFw03NTAxMDEwMDAwMDBaGA80MDk2MDEw
MTAwMDAwMFowITEfMB0GA1UEAwwWcmNnZW4gc2VsZiBzaWduZWQgY2VydDBZMBMG
ByqGSM49AgEGCCqGSM49AwEHA0IABDQGRlxUCoK3AxFH+9dfmzda2ucydRpP2IXe
yMX9mFt0p1njajXmKmpmV9JrNMjjhwxwv/oQEwdPud9a0ANYXsmjIDAeMBwGA1Ud
EQQVMBOCCWxvY2FsaG9zdIIGYXJjaGVyMAoGCCqGSM49BAMCA0kAMEYCIQDkTIQv
EeV0vsXSiwaorj0I/+Zk5j5W8dWtkk+2myqMlQIhAIovNRj0fdk6TrcLJfdyXPTa
DlljIQNJ6cCTK33ar8fJ
-----END CERTIFICATE-----";

#[tokio::main]
async fn main() -> Result<()> {
    let (layer, handle) = tracing_quiver::layer(CERTIFICATE).await?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(layer)
        .init();

    greet();

    tokio::time::sleep(Duration::from_secs(1)).await;
    handle.shutdown().await;

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
