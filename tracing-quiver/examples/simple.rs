use std::time::Duration;

use anyhow::Result;
use tracing::{debug_span, info, instrument, warn, Level};
use tracing_subscriber::{filter::Targets, prelude::*};

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
    let (quiver, handle) = tracing_quiver::builder()
        .with_server_cert(CERTIFICATE)
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
