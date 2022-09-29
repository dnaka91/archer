use std::{
    io::{Cursor, ErrorKind},
    net::SocketAddr,
    path::Path,
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use quinn::{Connecting, ConnectionError, Endpoint, NewConnection, RecvStream, ServerConfig};
use rustls::{Certificate, PrivateKey};
use tokio::fs;
use tokio_shutdown::Shutdown;
use tracing::{debug, error, info, instrument};
use unidirs::{Directories, UnifiedDirs};

use crate::{convert, net, storage::Database};

#[instrument(name = "quiver", skip_all)]
pub async fn run(shutdown: Shutdown, database: Database) -> Result<()> {
    let addr = SocketAddr::from(net::QUIVER_COLLECTOR);
    let (config, cert) = load_config().await?;
    let (endpoint, mut incoming) = Endpoint::server(config, addr)?;

    info!("listening on http://{}", endpoint.local_addr()?);
    info!("server certificate:\n{cert}");

    loop {
        let conn = tokio::select! {
            _ = shutdown.handle() => break,
            conn = incoming.next() => match conn {
                Some(conn) => conn,
                None => break,
            }
        };

        debug!(
            addr = %conn.remote_address(),
            "incoming connection"
        );

        let database = database.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(conn, database).await {
                error!(error = ?e, "failed handling connection");
            }
        });
    }

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
                rustls_pemfile::Item::PKCS8Key(key) => Ok(key),
                _ => bail!("not a key"),
            })
            .context("empty data")??;
        let cert_pem = String::from_utf8(cert_raw)?;

        (Certificate(cert), PrivateKey(key), cert_pem)
    } else {
        let (cert, cert_pem, key, key_pem) = generate_certificate()?;
        fs::create_dir_all(&data_dir).await?;
        fs::write(data_dir.join("cert.pem"), &cert_pem).await?;
        fs::write(data_dir.join("key.pem"), key_pem).await?;

        (cert, key, cert_pem)
    };

    let mut config = ServerConfig::with_single_cert(vec![cert], key)?;
    Arc::get_mut(&mut config.transport)
        .context("failed getting mutable reference to server transport")?
        .max_concurrent_bidi_streams(0_u8.into())
        .datagram_receive_buffer_size(None);

    Ok((config, cert_pem))
}

async fn load_file(path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
    match fs::read(path.as_ref()).await {
        Ok(buf) => Ok(Some(buf)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn generate_certificate() -> Result<(Certificate, String, PrivateKey, String)> {
    let cert = rcgen::generate_simple_self_signed(["localhost".to_owned(), "archer".to_owned()])?;

    Ok((
        Certificate(cert.serialize_der()?),
        cert.serialize_pem()?,
        PrivateKey(cert.serialize_private_key_der()),
        cert.serialize_private_key_pem(),
    ))
}

async fn handle_connection(conn: Connecting, database: Database) -> Result<()> {
    let NewConnection {
        connection,
        mut uni_streams,
        ..
    } = conn.await?;

    debug!(addr = %connection.remote_address(), "connection established");

    while let Some(stream) = uni_streams.next().await {
        let stream = match stream {
            Err(ConnectionError::ApplicationClosed(_) | ConnectionError::TimedOut) => return Ok(()),
            Err(e) => return Err(e.into()),
            Ok(s) => s,
        };

        debug!(addr = %connection.remote_address(), "incoming request");
        let database = database.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_request(stream, database).await {
                error!(error = ?e, "failed handling request");
            }
        });
    }

    Ok(())
}

async fn handle_request(recv: RecvStream, database: Database) -> Result<()> {
    let req = recv
        .read_to_end(64 * 1024)
        .await
        .context("failed reading request")?;

    let raw = snap::raw::Decoder::new().decompress_vec(&req)?;
    let span = rmp_serde::from_slice::<super::models::Span>(&raw)?;
    let span = convert::span_from_quiver(span);

    tokio::spawn(async move {
        if let Err(e) = database.save_spans(vec![span]).await {
            error!(error = ?e, "failed to save spans to DB");
        }
    });

    Ok(())
}
