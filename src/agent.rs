use std::net::{Ipv4Addr, SocketAddr};

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
        server::TProcessor,
    },
    zipkincore,
};
use tokio::net::UdpSocket;
use tokio_shutdown::Shutdown;
use tracing::{error, info, instrument};

use crate::{convert, storage::Database};

#[instrument(name = "agent", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 6831));
    let socket = UdpSocket::bind(addr).await?;
    info!(protocol = %"compact", "listening on http://{addr}");

    let compact = run_udp_server(
        shutdown.clone(),
        database.clone(),
        socket,
        |processor, input, output| {
            processor.process(
                &mut TCompactInputProtocol::new(input),
                &mut TCompactOutputProtocol::new(output),
            )
        },
    );

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 6832));
    let socket = UdpSocket::bind(addr).await?;
    info!(protocol = %"binary", "listening on http://{addr}");

    let binary = run_udp_server(shutdown, database, socket, |processor, input, output| {
        processor.process(
            &mut TBinaryInputProtocol::new(input, true),
            &mut TBinaryOutputProtocol::new(output, true),
        )
    });

    let (compact, binary) = tokio::try_join!(tokio::spawn(compact), tokio::spawn(binary))?;

    compact?;
    binary?;

    info!("server stopped");

    Ok(())
}

async fn run_udp_server(
    shutdown: Shutdown,
    database: Database,
    socket: UdpSocket,
    process: impl Fn(&AgentSyncProcessor<Handler>, &[u8], &mut [u8]) -> Result<(), thrift::Error>,
) -> Result<()> {
    let mut buf_in = vec![0u8; 65_000];
    let mut buf_out = vec![0u8; 65_000];

    let processor = AgentSyncProcessor::new(Handler(database));

    loop {
        let (len, addr) = tokio::select! {
            _ = shutdown.handle() => break,
            res = socket.recv_from(&mut buf_in) => match res {
                Ok(res) => res,
                Err(err) => {
                    error!(error = ?err, "failed receiving data");
                    continue;
                }
            },
        };

        buf_out.clear();

        if let Err(err) = (process)(&processor, &buf_in[..len], &mut buf_out) {
            error!(error = ?err, "failed to process request");
            continue;
        }

        if !buf_out.is_empty() {
            if let Err(err) = socket.send_to(&buf_out, addr).await {
                error!(error = ?err, "failed to send back response");
            }
        }
    }

    Ok(())
}

struct Handler(Database);

impl AgentSyncHandler for Handler {
    fn handle_emit_zipkin_batch(&self, _spans: Vec<zipkincore::Span>) -> thrift::Result<()> {
        Err("not implemented".into())
    }

    #[instrument(skip_all)]
    fn handle_emit_batch(&self, batch: jaeger::Batch) -> thrift::Result<()> {
        let db = self.0.clone();

        tokio::spawn(async move {
            for span in batch.spans {
                db.save_span(convert::span_from_thrift(span, Some(batch.process.clone())).unwrap())
                    .await
                    .unwrap();
            }
        });

        Ok(())
    }
}
