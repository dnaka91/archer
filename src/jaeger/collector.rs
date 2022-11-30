use std::{io::Read, net::SocketAddr};

use anyhow::Result;
use archer_http::{
    axum::{
        async_trait,
        body::{Bytes, HttpBody},
        extract::{rejection::BytesRejection, FromRequest, State},
        http::{Request, StatusCode},
        response::{IntoResponse, Response},
        routing::post,
        BoxError, Router, Server,
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
    tonic::{self, codegen::CompressionEncoding},
};
use archer_thrift::{jaeger::Batch, thrift::protocol::TBinaryInputProtocol};
use tokio_shutdown::Shutdown;
use tracing::{error, info, instrument, warn};

use crate::{convert, net, storage::Database};

#[instrument(name = "collector", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let (http, grpc) = tokio::try_join!(
        tokio::spawn(run_http(
            tracing::Span::current(),
            shutdown.clone(),
            database.clone(),
            SocketAddr::from(net::JAEGER_COLLECTOR_HTTP),
        )),
        tokio::spawn(run_grpc(
            tracing::Span::current(),
            shutdown,
            database,
            SocketAddr::from(net::JAEGER_COLLECTOR_GRPC),
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
    info!("listening on http://{addr}");

    let app = Router::new()
        .route("/api/traces", post(traces))
        .layer(ServiceBuilder::new().compression().trace_for_http())
        .with_state(database);

    Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown.handle())
        .await?;

    info!("server stopped");

    Ok(())
}

async fn traces(
    State(db): State<Database>,
    Thrift(batch): Thrift<Batch>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let spans = batch
        .spans
        .into_iter()
        .map(|span| convert::span_from_thrift(span, batch.process.clone()))
        .collect::<Result<Vec<_>>>()
        .map_err(|e| {
            error!(error = ?e, "failed converting spans");
            (StatusCode::BAD_REQUEST, e.to_string())
        })?;

    tokio::spawn(async move {
        if let Err(e) = db.save_spans(spans).await {
            error!(error = ?e, "failed to save spans to DB");
        }
    });

    Ok(StatusCode::ACCEPTED)
}

struct Thrift<T>(pub T);

#[async_trait]
impl<T, S, B> FromRequest<S, B> for Thrift<T>
where
    T: ThriftDeserialize,
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
    S: Send + Sync,
{
    type Rejection = ThriftRejection;

    async fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await?;
        let value = T::deserialize(&bytes[..])?;

        Ok(Self(value))
    }
}

#[derive(Debug, thiserror::Error)]
enum ThriftRejection {
    #[error("{0}")]
    Bytes(#[from] BytesRejection),
    #[error("Failed to parse the request body as Thrift message")]
    Decode(#[from] archer_thrift::thrift::Error),
}

impl IntoResponse for ThriftRejection {
    fn into_response(self) -> Response {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

trait ThriftDeserialize: Sized {
    fn deserialize<R>(data: R) -> archer_thrift::thrift::Result<Self>
    where
        R: Read;
}

impl ThriftDeserialize for archer_thrift::jaeger::Batch {
    fn deserialize<R>(data: R) -> archer_thrift::thrift::Result<Self>
    where
        R: Read,
    {
        let mut prot = TBinaryInputProtocol::new(data, true);
        archer_thrift::jaeger::read_batch(&mut prot)
    }
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
        .layer(ServiceBuilder::new().trace_for_grpc())
        .add_service(
            CollectorServiceServer::new(CollectorService(database))
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
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
        let api_v2::Batch { spans, process } =
            batch.ok_or_else(|| tonic::Status::invalid_argument("batch field missing"))?;
        let process = process
            .ok_or_else(|| tonic::Status::invalid_argument("process information missing"))?;
        let spans = spans
            .into_iter()
            .map(|mut span| {
                span.process.get_or_insert_with(|| process.clone());
                convert::span_from_proto(span)
            })
            .collect::<Result<Vec<_>>>()
            .map_err(|e| {
                warn!(error = ?e, "failed to convert spans");
                tonic::Status::invalid_argument(e.to_string())
            })?;
        let db = self.0.clone();

        tokio::spawn(async move {
            if let Err(e) = db.save_spans(spans).await {
                error!(error = ?e, "failed to save spans to DB");
            }
        });

        Ok(tonic::Response::new(PostSpansResponse::default()))
    }
}
