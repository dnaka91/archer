use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr},
};

use anyhow::Result;
use archer_http::{
    axum::{
        async_trait,
        body::{Bytes, HttpBody},
        extract::{FromRequest, RequestParts},
        http::StatusCode,
        response::{IntoResponse, Response},
        routing::post,
        BoxError, Extension, Router, Server,
    },
    tower::ServiceBuilder,
    tower_http::ServiceBuilderExt,
};
use archer_proto::{
    jaeger::api_v2::{
        self,
        collector_service_server::{self, CollectorServiceServer},
        PostSpansRequest, PostSpansResponse,
    },
    tonic,
};
use archer_thrift::{jaeger::Batch, thrift::protocol::TBinaryInputProtocol};
use tokio_shutdown::Shutdown;
use tracing::{info, instrument};

use crate::{convert, storage::Database};

#[instrument(name = "collector", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let (http, grpc) = tokio::try_join!(
        tokio::spawn(run_http(
            tracing::Span::current(),
            shutdown.clone(),
            database.clone(),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 14268)),
        )),
        tokio::spawn(run_grpc(
            tracing::Span::current(),
            shutdown,
            database,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 14250)),
        ))
    )?;

    http?;
    grpc?;

    Ok(())
}

#[instrument(name = "http", parent = parent, skip_all)]
async fn run_http(
    parent: tracing::Span,
    shutdown: Shutdown,
    database: Database,
    addr: SocketAddr,
) -> Result<()> {
    info!(protocol = %"http", "listening on http://{addr}");

    let app = Router::new().route("/api/traces", post(traces)).layer(
        ServiceBuilder::new()
            .trace_for_http()
            .layer(Extension(database)),
    );

    Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown.handle())
        .await?;

    info!("server stopped");

    Ok(())
}

async fn traces(
    Thrift(batch): Thrift<Batch>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    for span in batch.spans {
        db.save_span(convert::span_from_thrift(span, Some(batch.process.clone())).unwrap())
            .await
            .unwrap();
    }
    StatusCode::ACCEPTED
}

struct Thrift<T>(pub T);

#[async_trait]
impl<T, B> FromRequest<B> for Thrift<T>
where
    T: ThriftDeserialize,
    B: HttpBody + Send,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Rejection = ThriftRejection;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req).await.unwrap();
        let value = T::deserialize(&bytes[..]).unwrap();

        Ok(Self(value))
    }
}

enum ThriftRejection {}

impl IntoResponse for ThriftRejection {
    fn into_response(self) -> Response {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

trait ThriftDeserialize: Sized {
    fn deserialize<R>(data: R) -> Result<Self>
    where
        R: Read;
}

impl ThriftDeserialize for archer_thrift::jaeger::Batch {
    fn deserialize<R>(data: R) -> Result<Self>
    where
        R: Read,
    {
        let mut prot = TBinaryInputProtocol::new(data, true);
        Self::read_from_in_protocol(&mut prot).map_err(Into::into)
    }
}

#[instrument(name = "grpc", parent = parent, skip_all)]
async fn run_grpc(
    parent: tracing::Span,
    shutdown: Shutdown,
    database: Database,
    addr: SocketAddr,
) -> Result<()> {
    info!(protocol = %"grpc", "listening on http://{addr}");

    tonic::transport::Server::builder()
        .add_service(CollectorServiceServer::new(CollectorService(database)))
        .serve_with_shutdown(addr, shutdown.handle())
        .await?;

    info!("server stopped");

    Ok(())
}

struct CollectorService(Database);

#[tonic::async_trait]
impl collector_service_server::CollectorService for CollectorService {
    async fn post_spans(
        &self,
        request: tonic::Request<PostSpansRequest>,
    ) -> Result<tonic::Response<PostSpansResponse>, tonic::Status> {
        let PostSpansRequest { batch } = request.into_inner();
        let api_v2::Batch { spans, process } = batch.unwrap();
        let process = process.unwrap();
        let db = self.0.clone();

        tokio::spawn(async move {
            for mut span in spans {
                span.process.get_or_insert_with(|| process.clone());
                db.save_span(convert::span_from_proto(span).unwrap())
                    .await
                    .unwrap();
            }
        });

        Ok(tonic::Response::new(PostSpansResponse {}))
    }
}
