use std::net::SocketAddr;

use anyhow::Result;
use archer_http::{
    axum::{
        async_trait,
        body::{Bytes, HttpBody},
        extract::{rejection::BytesRejection, FromRequest, State},
        http::{header::CONTENT_TYPE, HeaderValue, Request, StatusCode},
        response::{IntoResponse, Response},
        routing::post,
        BoxError, Router, Server,
    },
    tower::ServiceBuilder,
    tower_http::ServiceBuilderExt,
};
use archer_proto::{
    opentelemetry::proto::{
        collector::trace::v1::{
            trace_service_server::{self, TraceServiceServer},
            ExportTraceServiceRequest, ExportTraceServiceResponse,
        },
        trace::v1::ResourceSpans,
    },
    prost::{DecodeError, Message},
    tonic::{self, codegen::CompressionEncoding},
};
use bytes::BytesMut;
use mime::Mime;
use tokio_shutdown::Shutdown;
use tracing::{error, info, instrument, warn};

use crate::{convert, models, net, storage::Database};

#[instrument(name = "otlp", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let (grpc, http) = tokio::try_join!(
        tokio::spawn(run_grpc(
            tracing::Span::current(),
            shutdown.clone(),
            database.clone(),
            SocketAddr::from(net::OTLP_COLLECTOR_GRPC)
        )),
        tokio::spawn(run_http(
            tracing::Span::current(),
            shutdown,
            database,
            SocketAddr::from(net::OTLP_COLLECTOR_HTTP)
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

    let app = Router::new()
        .route("/v1/traces", post(traces))
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
    Protobuf(request): Protobuf<ExportTraceServiceRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let spans = convert_resource_spans(request.resource_spans)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    tokio::spawn(async move {
        if let Err(e) = db.save_spans(spans).await {
            error!(error = ?e, "failed to save spans to DB");
        }
    });

    Ok(Protobuf(ExportTraceServiceResponse {
        partial_success: None,
    }))
}

struct Protobuf<T>(pub T);

#[async_trait]
impl<T, S, B> FromRequest<S, B> for Protobuf<T>
where
    T: Default + Message,
    B: HttpBody + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
    S: Send + Sync,
{
    type Rejection = ProtobufRejection;

    async fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
        if !protobuf_content_type(&req) {
            return Err(ProtobufRejection::MissingContentType);
        }

        let bytes = Bytes::from_request(req, state).await?;
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

fn protobuf_content_type<B>(req: &Request<B>) -> bool {
    req.headers()
        .get(CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .and_then(|ct| ct.parse::<Mime>().ok())
        .map_or(false, |ct| {
            ct.type_() == mime::APPLICATION && ct.subtype() == "x-protobuf"
        })
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
        .add_service(
            TraceServiceServer::new(TraceService(database))
                .accept_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Gzip),
        )
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
        let spans = convert_resource_spans(request.into_inner().resource_spans)
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        let db = self.0.clone();

        tokio::spawn(async move {
            if let Err(e) = db.save_spans(spans).await {
                error!(error = ?e, "failed to save spans to DB");
            }
        });

        Ok(tonic::Response::new(ExportTraceServiceResponse::default()))
    }
}

fn convert_resource_spans(resource_spans: Vec<ResourceSpans>) -> Result<Vec<models::Span>> {
    let span_len = convert::span_from_otlp_len(&resource_spans);

    resource_spans
        .into_iter()
        .try_fold(
            Vec::with_capacity(span_len),
            |mut acc, spans| match convert::span_from_otlp(spans) {
                Ok(spans) => {
                    acc.extend(spans);
                    Ok(acc)
                }
                Err(e) => {
                    warn!(error = ?e, "failed to convert spans");
                    Err(e)
                }
            },
        )
}
