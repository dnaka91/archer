use std::{
    borrow::Cow,
    io::Cursor,
    net::{Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use quinn::{ClientConfig, Endpoint, TransportConfig, VarInt};
use rustls::{Certificate, RootCertStore};
use tokio::{
    sync::{mpsc, oneshot},
    time,
};
use tracing::debug;

use crate::models;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to establish new stream")]
    CreateStream(#[from] quinn::ConnectionError),
    #[error("failed serializing data")]
    Serialize(#[from] postcard::Error),
    #[error("failed compressing data")]
    Compress(#[from] snap::Error),
    #[error("failed to send data over stream")]
    Write(#[from] quinn::WriteError),
}

struct Connection {
    receiver: mpsc::Receiver<Message>,
    endpoint: quinn::Endpoint,
    conn: Arc<quinn::Connection>,
    active_tasks: Arc<AtomicUsize>,
}

enum Message {
    SendSpan {
        span: Box<models::Span>,
        respond_to: oneshot::Sender<Result<(), Error>>,
    },
    Shutdown {
        max_wait: Duration,
        respond_to: oneshot::Sender<()>,
    },
}

impl Connection {
    fn new(
        receiver: mpsc::Receiver<Message>,
        endpoint: quinn::Endpoint,
        conn: quinn::Connection,
    ) -> Self {
        Self {
            receiver,
            endpoint,
            conn: Arc::new(conn),
            active_tasks: Arc::new(AtomicUsize::new(0)),
        }
    }

    async fn handle_message(&mut self, msg: Message) -> bool {
        match msg {
            Message::SendSpan { span, respond_to } => {
                let conn = Arc::clone(&self.conn);
                let active_tasks = Arc::clone(&self.active_tasks);

                active_tasks.fetch_add(1, Ordering::SeqCst);

                tokio::spawn(async move {
                    let result = async {
                        let mut send = conn.open_uni().await?;

                        let data = postcard::to_stdvec(&span)?;
                        let data = snap::raw::Encoder::new().compress_vec(&data)?;

                        send.write_all(&data).await?;
                        send.finish().await?;

                        Ok(())
                    };

                    respond_to.send(result.await).ok();
                    active_tasks.fetch_sub(1, Ordering::SeqCst);
                });

                false
            }
            Message::Shutdown {
                max_wait,
                respond_to,
            } => {
                let start = Instant::now();
                debug!("waiting for remaining tasks to finish");

                // TODO: not great to poll here, and if the task panics the counter will be off and
                // we fall back to the `max_wait` (which might be bad if the wait time is long).
                while start.elapsed() < max_wait && self.active_tasks.load(Ordering::SeqCst) > 0 {
                    time::sleep(Duration::from_millis(50)).await;
                }

                let waited = start.elapsed();
                debug!(?waited, timeout = waited > max_wait, "shutting down");

                self.conn.close(0u8.into(), b"done");
                self.endpoint.wait_idle().await;
                respond_to.send(()).ok();
                true
            }
        }
    }
}

async fn drive_connection(mut conn: Connection) {
    while let Some(msg) = conn.receiver.recv().await {
        if conn.handle_message(msg).await {
            break;
        }
    }
}

#[derive(Clone)]
pub struct Handle {
    sender: mpsc::Sender<Message>,
}

impl Handle {
    pub fn new(endpoint: quinn::Endpoint, conn: quinn::Connection) -> Self {
        let (sender, receiver) = mpsc::channel(16);
        let conn = Connection::new(receiver, endpoint, conn);
        tokio::spawn(drive_connection(conn));

        Self { sender }
    }

    pub async fn send_span(&self, span: models::Span) -> Result<(), Error> {
        let (send, recv) = oneshot::channel();
        let msg = Message::SendSpan {
            span: span.into(),
            respond_to: send,
        };

        if self.sender.send(msg).await.is_ok() {
            recv.await.expect("connection task has been destroyed")
        } else {
            Ok(())
        }
    }

    pub async fn shutdown(self, max_wait: Duration) {
        let (send, recv) = oneshot::channel();
        let msg = Message::Shutdown {
            max_wait,
            respond_to: send,
        };

        self.sender.send(msg).await.ok();
        recv.await.expect("connection task has been destroyed");
    }

    pub(crate) fn shutdown_blocking(&self) {
        let (send, recv) = oneshot::channel();
        let msg = Message::Shutdown {
            max_wait: Duration::from_secs(1),
            respond_to: send,
        };

        self.sender.blocking_send(msg).ok();
        recv.blocking_recv()
            .expect("connection task has been destroyed");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("I/O error happened")]
    Io(#[from] std::io::Error),
    #[error("failed loading certificate")]
    Webpki(#[from] webpki::Error),
    #[error("failed to connect to the server")]
    Connect(#[from] quinn::ConnectError),
    #[error("failed to complete connection to the server")]
    Connection(#[from] quinn::ConnectionError),
}

pub fn create_endpoint(cert_pem: &[u8]) -> Result<Endpoint, ConnectError> {
    let mut cert_pem = Cursor::new(cert_pem);
    let mut certs = RootCertStore::empty();

    for cert in rustls_pemfile::certs(&mut cert_pem)? {
        certs.add(&Certificate(cert))?;
    }

    let mut config = ClientConfig::with_root_certificates(certs);
    config.transport_config(Arc::new({
        let mut cfg = TransportConfig::default();
        cfg.max_concurrent_bidi_streams(0_u8.into())
            .max_concurrent_uni_streams(0_u8.into())
            .max_idle_timeout(Some(VarInt::from_u32(360_000).into()))
            .keep_alive_interval(Some(Duration::from_secs(30)));
        cfg
    }));

    let mut endpoint = Endpoint::client(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
    endpoint.set_default_client_config(config);

    Ok(endpoint)
}

pub async fn create_connection(
    endpoint: &quinn::Endpoint,
    addr: Option<SocketAddr>,
    name: Option<Cow<'_, str>>,
) -> Result<quinn::Connection, ConnectError> {
    let addr = addr.unwrap_or_else(|| (Ipv4Addr::LOCALHOST, 14000).into());
    let server_name = name.unwrap_or_else(|| "localhost".into());

    Ok(endpoint.connect(addr, &server_name)?.await?)
}
