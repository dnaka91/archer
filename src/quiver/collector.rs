use std::{
    io::{Cursor, ErrorKind},
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use quinn::{Connecting, ConnectionError, Endpoint, RecvStream, ServerConfig, VarInt};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::fs;
use tokio_shutdown::Shutdown;
use tracing::{debug, error, info, instrument};
use unidirs::{Directories, UnifiedDirs};

use crate::{convert, net, storage::Database};

#[instrument(name = "quiver", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let addr = SocketAddr::from(net::QUIVER_COLLECTOR);
    let (config, cert) = load_config().await?;
    let endpoint = Endpoint::server(config, addr)?;
    let mut tasks = Vec::new();

    info!("listening on http://{}", endpoint.local_addr()?);
    info!("server certificate:\n{cert}");

    loop {
        let conn = tokio::select! {
            () = shutdown.handle() => break,
            conn = endpoint.accept() => match conn {
                Some(conn) => conn,
                None => break,
            }
        };

        debug!(
            addr = %conn.remote_address(),
            "incoming connection"
        );

        let conn = conn.accept()?;
        let shutdown = shutdown.clone();
        let database = database.clone();

        tasks.push(tokio::spawn(async move {
            if let Err(e) = handle_connection(shutdown, conn, database).await {
                error!(error = ?e, "failed handling connection");
            }
        }));

        tasks.retain(|task| !task.is_finished());
    }

    for task in tasks {
        if let Err(e) = task.await {
            error!(error = ?e, "connection handler task panicked");
        }
    }

    endpoint.close(VarInt::from_u32(1), b"shutdown");

    Ok(())
}

async fn load_config() -> Result<(ServerConfig, String)> {
    let dirs = UnifiedDirs::simple("rocks", "dnaka91", env!("CARGO_PKG_NAME"))
        .default()
        .context("failed finding project directories")?;
    let data_dir = dirs.data_dir().join("quiver");

    let cert = load_file(data_dir.join("cert.pem")).await?;
    let key = load_file(data_dir.join("key.pem")).await?;

    let (cert, key, cert_pem) = if let Some((cert_raw, key_raw)) = cert.zip(key) {
        let cert = rustls_pemfile::read_one(&mut Cursor::new(&cert_raw))?
            .map(|item| match item {
                rustls_pemfile::Item::X509Certificate(cert) => Ok(cert),
                _ => bail!("not a certificate"),
            })
            .context("empty data")??;
        let key = rustls_pemfile::read_one(&mut Cursor::new(key_raw))?
            .map(|item| match item {
                rustls_pemfile::Item::Pkcs8Key(key) => Ok(key),
                _ => bail!("not a key"),
            })
            .context("empty data")??;
        let cert_pem = String::from_utf8(cert_raw)?;

        (cert, key, cert_pem)
    } else {
        let (cert, cert_pem, key, key_pem) = generate_certificate()?;
        fs::create_dir_all(&data_dir).await?;
        fs::write(data_dir.join("cert.pem"), &cert_pem).await?;
        fs::write(data_dir.join("key.pem"), key_pem).await?;

        (cert, key, cert_pem)
    };

    let mut config = ServerConfig::with_single_cert(vec![cert], key.into())?;
    Arc::get_mut(&mut config.transport)
        .context("failed getting mutable reference to server transport")?
        .max_concurrent_bidi_streams(0_u8.into())
        .datagram_receive_buffer_size(None)
        .max_idle_timeout(Some(VarInt::from_u32(360_000).into()))
        .keep_alive_interval(Some(Duration::from_secs(30)));

    Ok((config, cert_pem))
}

async fn load_file(path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
    match fs::read(path.as_ref()).await {
        Ok(buf) => Ok(Some(buf)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn generate_certificate() -> Result<(
    CertificateDer<'static>,
    String,
    PrivatePkcs8KeyDer<'static>,
    String,
)> {
    let cert = rcgen::generate_simple_self_signed(["localhost".to_owned(), "archer".to_owned()])?;

    Ok((
        cert.cert.der().to_owned(),
        cert.cert.pem(),
        cert.key_pair.serialize_der().into(),
        cert.key_pair.serialize_pem(),
    ))
}

async fn handle_connection(shutdown: Shutdown, conn: Connecting, database: Database) -> Result<()> {
    let connection = conn.await?;
    let mut tasks = Vec::new();

    debug!(addr = %connection.remote_address(), "connection established");

    loop {
        let stream = tokio::select! {
            () = shutdown.handle() => {
                break;
            }
            stream = connection.accept_uni() => stream,
        };

        let stream = match stream {
            Err(ConnectionError::ApplicationClosed(_) | ConnectionError::TimedOut) => return Ok(()),
            Err(e) => return Err(e.into()),
            Ok(s) => s,
        };

        debug!(addr = %connection.remote_address(), "incoming request");
        let database = database.clone();

        tasks.push(tokio::spawn(async move {
            if let Err(e) = handle_request(stream, database).await {
                error!(error = ?e, "failed handling request");
            }
        }));

        tasks.retain(|task| !task.is_finished());
    }

    for task in tasks {
        if let Err(e) = task.await {
            error!(error = ?e, "request handler task panicked");
        }
    }

    Ok(())
}

async fn handle_request(mut recv: RecvStream, database: Database) -> Result<()> {
    let req = tokio::time::timeout(Duration::from_secs(5), recv.read_to_end(64 * 1024))
        .await
        .context("read timeout")?
        .context("failed reading request")?;

    let raw = snap::raw::Decoder::new().decompress_vec(&req)?;
    let span = postcard::from_bytes::<super::models::Span>(&raw)?;
    let span = convert::span_from_quiver(span);

    tokio::spawn(async move {
        if let Err(e) = database.save_spans(vec![span]).await {
            error!(error = ?e, "failed to save spans to DB");
        }
    });

    Ok(())
}
