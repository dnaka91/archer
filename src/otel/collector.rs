use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Result;
use archer_http::{
    axum::{
        async_trait,
        body::{Bytes, HttpBody},
        extract::{rejection::BytesRejection, FromRequest, RequestParts},
        http::{header::CONTENT_TYPE, HeaderValue, StatusCode},
        response::{IntoResponse, Response},
        routing::post,
        BoxError, Extension, Router, Server,
    },
    tower::ServiceBuilder,
    tower_http::ServiceBuilderExt,
};
use archer_proto::{
    opentelemetry::proto::collector::trace::v1::{
        trace_service_server::{self, TraceServiceServer},
        ExportTraceServiceRequest, ExportTraceServiceResponse,
    },
    prost::{DecodeError, Message},
    tonic,
};
use bytes::BytesMut;
use mime::Mime;
use tokio_shutdown::Shutdown;
use tracing::{info, instrument};

use crate::storage::Database;

#[instrument(name = "otlp", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let (grpc, http) = tokio::try_join!(
        tokio::spawn(run_grpc(
            tracing::Span::current(),
            shutdown.clone(),
            database.clone(),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 4317))
        )),
        tokio::spawn(run_http(
            tracing::Span::current(),
            shutdown,
            database,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 4318))
        ))
    )?;

    grpc?;
    http?;

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

    let app = Router::new().route("/v1/traces", post(traces)).layer(
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
    Protobuf(trace): Protobuf<ExportTraceServiceRequest>,
    Extension(db): Extension<Database>,
) -> impl IntoResponse {
    Protobuf(ExportTraceServiceResponse {
        partial_success: None,
    })
}

struct Protobuf<T>(pub T);

#[async_trait]
impl<T, B> FromRequest<B> for Protobuf<T>
where
    T: Default + Message,
    B: HttpBody + Send,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Rejection = ProtobufRejection;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        if !protobuf_content_type(req) {
            return Err(ProtobufRejection::MissingContentType);
        }

        let bytes = Bytes::from_request(req).await?;
        let value = T::decode(bytes)?;

        Ok(Self(value))
    }
}

impl<T> IntoResponse for Protobuf<T>
where
    T: Message,
{
    fn into_response(self) -> Response {
        let mut buf = BytesMut::new();
        match self.0.encode(&mut buf) {
            Ok(()) => (
                [(
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/x-protobuf"),
                )],
                buf.freeze(),
            )
                .into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(
                    CONTENT_TYPE,
                    HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()),
                )],
                err.to_string(),
            )
                .into_response(),
        }
    }
}

fn protobuf_content_type<B>(req: &RequestParts<B>) -> bool {
    req.headers()
        .get(CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .and_then(|ct| ct.parse::<Mime>().ok())
        .map(|ct| ct.type_() == mime::APPLICATION && ct.subtype() == "x-protobuf")
        .unwrap_or(false)
}

#[derive(Debug, thiserror::Error)]
enum ProtobufRejection {
    #[error("Expected request with `Content-Type: application/x-protobuf`")]
    MissingContentType,
    #[error("{0}")]
    Bytes(#[from] BytesRejection),
    #[error("Failed to parse the request body as Protobuf message")]
    Decode(#[from] DecodeError),
}

impl IntoResponse for ProtobufRejection {
    fn into_response(self) -> Response {
        match self {
            Self::MissingContentType => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, self.to_string()).into_response()
            }
            Self::Bytes(bytes) => bytes.into_response(),
            Self::Decode(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
        }
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
