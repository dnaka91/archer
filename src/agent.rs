use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Result;
use archer_thrift::{
    agent::{AgentSyncHandler, AgentSyncProcessor},
    jaeger,
    thrift::{
        self,
        protocol::{TCompactInputProtocol, TCompactOutputProtocol},
        server::TProcessor,
    },
    zipkincore,
};
use tokio::net::UdpSocket;
use tokio_shutdown::Shutdown;
use tracing::{info, instrument};

use crate::{convert, storage::Database};

#[instrument(name = "agent", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 6831));
    info!("listening on http://{addr}");

    let socket = UdpSocket::bind(addr).await?;
    let mut buf_in = vec![0u8; 65_000];
    let mut buf_out = vec![0u8; 65_000];

    let processor = AgentSyncProcessor::new(Handler(database));

    loop {
        let (len, addr) = tokio::select! {
            _ = shutdown.handle() => break,
            res = socket.recv_from(&mut buf_in) => res?,
        };

        buf_out.clear();

        let mut input = TCompactInputProtocol::new(&buf_in[..len]);
        let mut output = TCompactOutputProtocol::new(&mut buf_out);

        processor.process(&mut input, &mut output)?;

        if !buf_out.is_empty() {
            socket.send_to(&buf_out, addr).await?;
        }
    }

    info!("server stopped");

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
                db.save_span(convert::span_from_thrift(span, Some(batch.process.clone())))
                    .await
                    .unwrap();
            }
        });

        Ok(())
    }
}
