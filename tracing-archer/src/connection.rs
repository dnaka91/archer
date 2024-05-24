use std::{
    borrow::Cow,
    io::Cursor,
    net::{Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use quinn::{ClientConfig, Endpoint, TransportConfig, VarInt};
use rustls::RootCertStore;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time,
};
use tracing::{debug, warn};

use crate::models;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to establish new connection")]
    CreateConnection(#[from] quinn::ConnectError),
    #[error("failed to establish new stream")]
    CreateStream(#[from] quinn::ConnectionError),
    #[error("failed serializing data")]
    Serialize(#[from] postcard::Error),
    #[error("failed compressing data")]
    Compress(#[from] snap::Error),
    #[error("failed to send data over stream")]
    Write(#[from] quinn::WriteError),
    #[error("failed to close stream")]
    CloseStream(#[from] quinn::ClosedStream),
}

struct Connection {
    receiver: mpsc::Receiver<Message>,
    addr: SocketAddr,
    server_name: Cow<'static, str>,
    connected: Arc<AtomicBool>,
    endpoint: quinn::Endpoint,
    conn: Arc<quinn::Connection>,
    tasks: mpsc::UnboundedSender<JoinHandle<()>>,
    handle: Option<JoinHandle<()>>,
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
    async fn new(
        receiver: mpsc::Receiver<Message>,
        endpoint: quinn::Endpoint,
        addr: Option<SocketAddr>,
        name: Option<Cow<'static, str>>,
    ) -> Result<Self, Error> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let addr = addr.unwrap_or_else(|| (Ipv4Addr::LOCALHOST, 14000).into());
        let server_name = name.unwrap_or_else(|| "localhost".into());

        let conn = endpoint.connect(addr, &server_name)?.await?;

        let handle = tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                if let Err(e) = task.await {
                    warn!(error = ?e, "sender task panicked");
                }
            }
        });

        Ok(Self {
            receiver,
            addr,
            server_name,
            connected: Arc::new(AtomicBool::new(false)),
            endpoint,
            conn: Arc::new(conn),
            tasks: tx,
            handle: Some(handle),
        })
    }

    async fn handle_message(&mut self, msg: Message) -> bool {
        match msg {
            Message::SendSpan { span, respond_to } => {
                if !self.connected.load(Ordering::SeqCst) {
                    let result = async {
                        let connecting = self.endpoint.connect(self.addr, &self.server_name)?;
                        let connection = connecting.await?;
                        self.conn = Arc::new(connection);
                        self.connected.store(true, Ordering::SeqCst);
                        Ok(())
                    };

                    if let Err(e) = result.await {
                        respond_to.send(Err(e)).ok();
                        return false;
                    }
                }

                let connected = Arc::clone(&self.connected);
                let conn = Arc::clone(&self.conn);

                self.tasks
                    .send(tokio::spawn(async move {
                        let result = async {
                            let data = postcard::to_stdvec(&span)?;
                            let data = snap::raw::Encoder::new().compress_vec(&data)?;

                            let sent = async {
                                let mut send = conn.open_uni().await?;
                                send.write_all(&data).await?;
                                send.finish()?;
                                Ok::<_, Error>(())
                            }
                            .await;

                            if sent.is_err() {
                                connected.store(false, Ordering::SeqCst);
                                sent?;
                            }

                            Ok(())
                        }
                        .await;

                        respond_to.send(result).ok();
                    }))
                    .ok();

                false
            }
            Message::Shutdown {
                max_wait,
                respond_to,
            } => {
                let start = Instant::now();
                debug!("waiting for remaining tasks to finish");

                if let Some(handle) = self.handle.take() {
                    if let Ok(Err(e)) = time::timeout(max_wait, handle).await {
                        warn!(error = ?e, "background task panicked");
                    }
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
    pub async fn new(
        endpoint: quinn::Endpoint,
        addr: Option<SocketAddr>,
        name: Option<Cow<'static, str>>,
    ) -> Result<Self, Error> {
        let (sender, receiver) = mpsc::channel(16);
        let conn = Connection::new(receiver, endpoint, addr, name).await?;
        tokio::spawn(drive_connection(conn));

        Ok(Self { sender })
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
    #[error("failed adding certificate to rustls")]
    Rustls(#[from] rustls::Error),
    #[error("failed creating client configuration for rustls")]
    RustlsClient(#[from] rustls::client::VerifierBuilderError),
}

pub fn create_endpoint(cert_pem: &[u8]) -> Result<Endpoint, ConnectError> {
    let mut cert_pem = Cursor::new(cert_pem);
    let mut certs = RootCertStore::empty();

    for cert in rustls_pemfile::certs(&mut cert_pem) {
        certs.add(cert?)?;
    }

    let mut config = ClientConfig::with_root_certificates(Arc::new(certs))?;
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
