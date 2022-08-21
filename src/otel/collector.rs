use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Result;
use archer_proto::{
    opentelemetry::proto::collector::trace::v1::{
        trace_service_server::{self, TraceServiceServer},
        ExportTraceServiceRequest, ExportTraceServiceResponse,
    },
    tonic,
};
use tokio_shutdown::Shutdown;
use tracing::{info, instrument};

use crate::storage::Database;

#[instrument(name = "otlp", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let (grpc,) = tokio::try_join!(tokio::spawn(run_grpc(
        tracing::Span::current(),
        shutdown,
        database,
        SocketAddr::from((Ipv4Addr::LOCALHOST, 4317))
    )))?;

    grpc?;

    Ok(())
}

#[instrument(name = "grpc", parent = parent, skip_all)]
async fn run_grpc(
    parent: tracing::Span,
    shutdown: Shutdown,
    database: Database,
    addr: SocketAddr,
) -> Result<()> {
    info!("listening on http://{addr}");

    tonic::transport::Server::builder()
        .add_service(TraceServiceServer::new(TraceService(database)))
        .serve_with_shutdown(addr, shutdown.handle())
        .await?;

    info!("server stopped");

    Ok(())
}

struct TraceService(Database);

#[tonic::async_trait]
impl trace_service_server::TraceService for TraceService {
    async fn export(
        &self,
        request: tonic::Request<ExportTraceServiceRequest>,
    ) -> Result<tonic::Response<ExportTraceServiceResponse>, tonic::Status> {
        let ExportTraceServiceRequest { resource_spans } = request.into_inner();

        tokio::spawn(async { for span in resource_spans {} });

        Ok(tonic::Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}
