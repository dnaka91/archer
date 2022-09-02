// #![deny(rust_2018_idioms, clippy::all, clippy::pedantic)]
// #![warn(clippy::expect_used, clippy::unwrap_used)]
#![allow(clippy::missing_errors_doc)]

use std::{
    borrow::Cow,
    io::Cursor,
    marker::PhantomData,
    net::{Ipv4Addr, SocketAddr},
    num::{NonZeroU128, NonZeroU64},
    sync::Arc,
    thread::Thread,
};

use once_cell::unsync::Lazy;
use quanta::{Clock, Instant};
use quinn::{ClientConfig, Connection, Endpoint, NewConnection};
use rustls::{Certificate, RootCertStore};
use time::{Duration, OffsetDateTime};
use tracing::{field::Visit, span, Metadata, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

mod models;

pub struct QuiverLayer<S> {
    connection: Connection,
    clock: Clock,
    resource: Resource,
    _inner: PhantomData<S>,
}

struct Timings {
    busy: Duration,
    idle: Duration,
    last: Instant,
}

impl Timings {
    fn new(clock: &Clock) -> Self {
        Self {
            busy: Duration::ZERO,
            idle: Duration::ZERO,
            last: clock.now(),
        }
    }
}

struct SpanBuilder {
    trace_id: NonZeroU128,
    span_id: NonZeroU64,
    name: &'static str,
    parent: Option<models::Reference>,
    start_time: OffsetDateTime,
    end_time: OffsetDateTime,
    location: Option<models::Location<'static>>,
    thread: Thread,
    thread_id: Option<u64>,
    tags: Vec<models::Tag<'static>>,
    logs: Vec<models::Log<'static>>,
}

impl SpanBuilder {
    fn new(trace_id: NonZeroU128, meta: &Metadata<'static>) -> Self {
        let now = OffsetDateTime::now_utc();

        Self {
            trace_id,
            span_id: rand::random(),
            name: meta.name(),
            parent: None,
            start_time: now,
            end_time: now,
            location: location_from_meta(meta),
            thread: std::thread::current(),
            thread_id: None,
            tags: Vec::new(),
            logs: Vec::new(),
        }
    }

    fn finish(mut self) -> Self {
        self.end_time = OffsetDateTime::now_utc();
        self
    }
}

fn location_from_meta<'a>(meta: &Metadata<'a>) -> Option<models::Location<'a>> {
    Some(models::Location {
        filepath: meta.file()?.into(),
        namespace: meta.module_path()?.into(),
        lineno: meta.line()?,
    })
}

#[derive(Clone)]
struct Resource {
    name: Arc<str>,
    version: Arc<str>,
}

impl Resource {
    fn new() -> Self {
        Self {
            name: "".into(),
            version: "".into(),
        }
    }
}

impl<S> Layer<S> for QuiverLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        !metadata.target().starts_with("quinn::") && !metadata.target().starts_with("quinn_proto::")
    }

    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        thread_local! {
            static THREAD_ID: Lazy<NonZeroU64> = Lazy::new(|| {
                let id = format!("{:?}", std::thread::current().id());
                id.trim_start_matches("ThreadId(")
                    .trim_end_matches(')')
                    .parse()
                    .expect("thread ID should parse as an integer")
            });
        }

        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();

        if extensions.get_mut::<Timings>().is_none() {
            extensions.insert(Timings::new(&self.clock));
        }

        if extensions.get_mut::<SpanBuilder>().is_none() {
            let trace_id = span
                .scope()
                .from_root()
                .next()
                .filter(|root| root.id() != span.id())
                .and_then(|root| root.extensions().get::<SpanBuilder>().map(|b| b.trace_id))
                .unwrap_or_else(rand::random);

            let parent = span
                .scope()
                .nth(1)
                .and_then(|parent| {
                    parent
                        .extensions()
                        .get::<SpanBuilder>()
                        .map(|b| (b.trace_id, b.span_id))
                })
                .map(|(trace_id, span_id)| models::Reference {
                    ty: models::RefType::ChildOf,
                    trace_id,
                    span_id,
                });

            let mut builder = SpanBuilder::new(trace_id, span.metadata());
            builder.parent = parent;
            builder.tags = Vec::with_capacity(attrs.fields().len());
            attrs.record(&mut SpanAttributeVisitor(&mut builder.tags));

            THREAD_ID.with(|id| {
                builder.thread_id = Some(id.get());
            });

            extensions.insert(builder);
        }
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();

        if let Some(builder) = extensions.get_mut::<SpanBuilder>() {
            values.record(&mut SpanAttributeVisitor(&mut builder.tags));
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.lookup_current() {
            let mut extensions = span.extensions_mut();

            if let Some(builder) = extensions.get_mut::<SpanBuilder>() {
                let mut log = models::Log {
                    timestamp: OffsetDateTime::now_utc(),
                    level: match *event.metadata().level() {
                        tracing::Level::TRACE => models::LogLevel::Trace,
                        tracing::Level::DEBUG => models::LogLevel::Debug,
                        tracing::Level::INFO => models::LogLevel::Info,
                        tracing::Level::WARN => models::LogLevel::Warn,
                        tracing::Level::ERROR => models::LogLevel::Error,
                    },
                    target: event.metadata().target().into(),
                    location: location_from_meta(event.metadata()),
                    fields: Vec::with_capacity(event.fields().count()),
                };

                event.record(&mut SpanAttributeVisitor(&mut log.fields));
                builder.logs.push(log);
            }
        }
    }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();

        if let Some(timings) = extensions.get_mut::<Timings>() {
            let now = self.clock.now();
            timings.idle += now.duration_since(timings.last);
            timings.last = now;
        }
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();

        if let Some(timings) = extensions.get_mut::<Timings>() {
            let now = self.clock.now();
            timings.busy += now.duration_since(timings.last);
            timings.last = now;
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("span not found");
        let mut extensions = span.extensions_mut();

        let builder = extensions
            .remove::<SpanBuilder>()
            .expect("span builder extension missing")
            .finish();
        let timings = extensions
            .remove::<Timings>()
            .expect("timings extension missing");

        let open_uni = self.connection.open_uni();
        let resource = self.resource.clone();

        tokio::spawn(async move {
            let mut send = open_uni.await.unwrap();

            let data = models::Span {
                trace_id: builder.trace_id,
                span_id: builder.span_id,
                operation_name: builder.name.into(),
                flags: 1,
                references: builder.parent.into_iter().collect(),
                start: builder.start_time,
                duration: builder.end_time - builder.start_time,
                location: builder.location,
                timing: models::Timing {
                    busy: timings.busy,
                    idle: timings.idle,
                },
                thread: builder
                    .thread_id
                    .zip(builder.thread.name())
                    .map(|(id, name)| models::Thread {
                        id,
                        name: name.into(),
                    }),
                tags: builder.tags,
                logs: builder.logs,
                process: models::Process {
                    service: resource.name,
                    version: resource.version,
                    tags: vec![],
                },
            };

            let data = rmp_serde::to_vec(&data).unwrap();
            let data = snap::raw::Encoder::new().compress_vec(&data).unwrap();

            send.write_all(&data).await.unwrap();
            send.finish().await.unwrap();
        });
    }
}

pub async fn layer<S>(
    cert_pem: impl Into<Cow<'static, str>>,
) -> Result<(QuiverLayer<S>, Handle), BuildLayerError> {
    builder().with_server_cert(cert_pem).build().await
}

pub struct Handle {
    endpoint: Endpoint,
    connection: Connection,
}

impl Handle {
    pub async fn shutdown(self) {
        self.connection.close(0u8.into(), b"done");
        self.endpoint.wait_idle().await;
    }
}

#[derive(Default)]
pub struct Builder {
    cert: Option<Cow<'static, str>>,
    addr: Option<SocketAddr>,
    name: Option<Cow<'static, str>>,
    clock: Option<Clock>,
    resource: Option<Resource>,
}

impl Builder {
    #[must_use]
    pub fn with_server_cert(mut self, cert: impl Into<Cow<'static, str>>) -> Self {
        self.cert = Some(cert.into());
        self
    }

    #[must_use]
    pub fn with_server_addr(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.addr = Some(addr.into());
        self
    }

    #[must_use]
    pub fn with_server_name(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = Some(clock);
        self
    }

    #[must_use]
    pub fn with_resource(
        mut self,
        name: impl Into<Cow<'static, str>>,
        version: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.resource = Some(Resource {
            name: name.into().into(),
            version: version.into().into(),
        });
        self
    }

    pub async fn build<S>(self) -> Result<(QuiverLayer<S>, Handle), BuildLayerError> {
        let cert_pem = self.cert.ok_or(BuildLayerError::MissingCertificate)?;
        let mut cert_pem = Cursor::new(cert_pem.as_bytes());
        let mut certs = RootCertStore::empty();

        for cert in rustls_pemfile::certs(&mut cert_pem)? {
            certs.add(&Certificate(cert))?;
        }

        let mut config = ClientConfig::with_root_certificates(certs);
        Arc::get_mut(&mut config.transport)
            .expect("failed getting mutable reference to client transport")
            .max_concurrent_bidi_streams(0_u8.into())
            .max_concurrent_uni_streams(0_u8.into());

        let mut endpoint = Endpoint::client(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))?;
        endpoint.set_default_client_config(config);

        let addr = self
            .addr
            .unwrap_or_else(|| (Ipv4Addr::LOCALHOST, 14000).into());
        let server_name = self.name.unwrap_or_else(|| "localhost".into());

        let NewConnection { connection, .. } = endpoint.connect(addr, &server_name)?.await?;

        let layer = QuiverLayer {
            connection: connection.clone(),
            clock: self.clock.unwrap_or_else(Clock::new),
            resource: self.resource.unwrap_or_else(Resource::new),
            _inner: PhantomData,
        };

        let handle = Handle {
            endpoint,
            connection,
        };

        Ok((layer, handle))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuildLayerError {
    #[error("the server certificate must be specified")]
    MissingCertificate,
    #[error("I/O error happened: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed loading certificate: {0}")]
    Webpki(#[from] webpki::Error),
    #[error("failed to connect to the server: {0}")]
    Connect(#[from] quinn::ConnectError),
    #[error("failed to complete connection to the server: {0}")]
    Connection(#[from] quinn::ConnectionError),
}

#[must_use]
pub fn builder() -> Builder {
    Builder::default()
}

struct SpanAttributeVisitor<'a>(&'a mut Vec<models::Tag<'static>>);

impl<'a> Visit for SpanAttributeVisitor<'a> {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::F64(value),
        });
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::I64(value),
        });
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::Bool(value),
        });
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::String(value.to_owned().into()),
        });
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::String(format!("{value:?}").into()),
        });
    }
}
