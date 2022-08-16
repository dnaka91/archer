use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Result;
use axum::{http::StatusCode, response::IntoResponse, routing::post, Router, Server};
use tokio_shutdown::Shutdown;
use tower_http::trace::TraceLayer;
use tracing::{info, instrument};

#[instrument(name = "collector", skip_all)]
pub async fn run(shutdown: Shutdown) -> Result<()> {
    let app = Router::new()
        .route("/api/traces", post(traces))
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 14268));
    info!("listening on http://{addr}");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown.handle())
        .await?;

    info!("server stopped");

    Ok(())
}

async fn traces() -> impl IntoResponse {
    StatusCode::ACCEPTED
}
