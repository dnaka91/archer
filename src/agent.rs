use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Result;
use tokio_shutdown::Shutdown;
use tracing::{info, instrument};

#[instrument(name = "agent", skip_all)]
pub async fn run(shutdown: Shutdown) -> Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 6831));
    info!("listening on http://{addr}");

    shutdown.handle().await;

    info!("server stopped");

    Ok(())
}
