use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr},
};

use anyhow::Result;
use archer_thrift::{jaeger::Batch, thrift::protocol::TBinaryInputProtocol};
use axum::{
    async_trait,
    body::{Bytes, HttpBody},
    extract::{FromRequest, RequestParts},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    BoxError, Extension, Router, Server,
};
use tokio_shutdown::Shutdown;
use tower::ServiceBuilder;
use tower_http::ServiceBuilderExt;
use tracing::{info, instrument};

use crate::{convert, storage::Database};

#[instrument(name = "collector", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let app = Router::new().route("/api/traces", post(traces)).layer(
        ServiceBuilder::new()
            .trace_for_http()
            .layer(Extension(database)),
    );

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 14268));
    info!("listening on http://{addr}");

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
