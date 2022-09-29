use std::{net::SocketAddr, time::Instant};

use anyhow::Result;
use archer_thrift::{
    agent::{AgentSyncHandler, AgentSyncProcessor},
    jaeger,
    thrift::{
        self,
        protocol::{
            TBinaryInputProtocol, TBinaryOutputProtocol, TCompactInputProtocol,
            TCompactOutputProtocol,
        },
    },
};
use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use tokio::net::UdpSocket;
use tokio_shutdown::Shutdown;
use tokio_util::{codec::BytesCodec, udp::UdpFramed};
use tracing::{debug_span, error, info, instrument, warn, Span};

use crate::{convert, net, storage::Database};

#[instrument(name = "agent", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let (compact, binary) = tokio::try_join!(
        tokio::spawn(run_compact(
            Span::current(),
            shutdown.clone(),
            database.clone(),
            SocketAddr::from(net::JAEGER_AGENT_COMPACT),
        )),
        tokio::spawn(run_binary(
            Span::current(),
            shutdown,
            database,
            SocketAddr::from(net::JAEGER_AGENT_BINARY),
        )),
    )?;

    compact?;
    binary?;

    Ok(())
}

#[instrument(name = "compact", parent = parent, skip_all)]
async fn run_compact(
    parent: Span,
    shutdown: Shutdown,
    database: Database,
    addr: SocketAddr,
) -> Result<()> {
    let socket = UdpSocket::bind(addr).await?;
    info!("listening on http://{addr}");

    run_udp_server(shutdown, database, socket, |processor, input, output| {
        processor.process(
            &mut TCompactInputProtocol::new(input),
            &mut TCompactOutputProtocol::new(output),
        )
    })
    .await;

    info!("server stopped");

    Ok(())
}

#[instrument(name = "binary", parent = parent, skip_all)]
async fn run_binary(
    parent: Span,
    shutdown: Shutdown,
    database: Database,
    addr: SocketAddr,
) -> Result<()> {
    let socket = UdpSocket::bind(addr).await?;
    info!("listening on http://{addr}");

    run_udp_server(shutdown, database, socket, |processor, input, output| {
        processor.process(
            &mut TBinaryInputProtocol::new(input, true),
            &mut TBinaryOutputProtocol::new(output, true),
        )
    })
    .await;

    info!("server stopped");

    Ok(())
}

async fn run_udp_server(
    shutdown: Shutdown,
    database: Database,
    socket: UdpSocket,
    process: impl Fn(&AgentSyncProcessor<Handler>, &[u8], &mut [u8]) -> Result<(), thrift::Error>,
) {
    let mut framed = UdpFramed::new(socket, BytesCodec::new());
    let mut output = BytesMut::new();
    let processor = AgentSyncProcessor::new(Handler(database));

    loop {
        let (frame, addr) = tokio::select! {
            _ = shutdown.handle() => break,
            res = framed.next() => match res {
                Some(Ok(res)) => res,
                Some(Err(err)) => {
                    error!(error = ?err, "failed receiving data");
                    continue;
                }
                None => break,
            },
        };

        let success = debug_span!(parent: None, "request").in_scope(|| {
            let now = Instant::now();
            tracing::debug!("started processing request");

            if let Err(err) = (process)(&processor, &frame, &mut output) {
                error!(error = ?err, "failed to process request");
                return false;
            }

            let latency = format!("{} ms", now.elapsed().as_millis());
            tracing::debug!(%latency, "finished processing request");

            true
        });

        if !success {
            continue;
        }

        let output = output.split().freeze();

        if !output.is_empty() {
            if let Err(err) = framed.send((output, addr)).await {
                error!(error = ?err, "failed to send back response");
            }
        }
    }
}

struct Handler(Database);

impl AgentSyncHandler for Handler {
    #[instrument(skip_all)]
    fn handle_emit_batch(&self, batch: jaeger::Batch) -> thrift::Result<()> {
        let spans = batch
            .spans
            .into_iter()
            .map(|span| convert::span_from_thrift(span, batch.process.clone()))
            .collect::<Result<Vec<_>>>()
            .map_err(|e| {
                warn!(error = ?e, "failed converting spans");
                thrift::Error::User(e.into())
            })?;
        let db = self.0.clone();

        tokio::spawn(async move {
            if let Err(e) = db.save_spans(spans).await {
                error!(error = ?e, "failed to save spans to DB");
            }
        });

        Ok(())
    }
}
