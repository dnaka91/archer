// #![deny(rust_2018_idioms, clippy::all, clippy::pedantic)]
// #![warn(clippy::expect_used, clippy::unwrap_used)]
#![allow(clippy::missing_errors_doc)]

use std::{
    borrow::Cow,
    marker::PhantomData,
    net::SocketAddr,
    num::{NonZeroU128, NonZeroU64},
    sync::Arc,
    thread::Thread,
};

use once_cell::unsync::Lazy;
use quanta::{Clock, Instant};
use time::{Duration, OffsetDateTime};
use tracing::{error, field::Visit, span, Metadata, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

pub use crate::connection::{ConnectError, Error};

mod connection;
mod models;

pub struct QuiverLayer<S> {
    connection: connection::Handle,
    clock: Clock,
    resource: Resource,
    _inner: PhantomData<S>,
}

impl<S> Drop for QuiverLayer<S> {
    fn drop(&mut self) {
        self.connection.shutdown_blocking();
    }
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
    location: Option<models::Location>,
    thread: Thread,
    thread_id: Option<u64>,
    tags: Vec<models::Tag>,
    logs: Vec<models::Log>,
    follows: Vec<models::Reference>,
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
            follows: Vec::new(),
        }
    }

    fn finish(mut self) -> Self {
        self.end_time = OffsetDateTime::now_utc();
        self
    }
}

fn location_from_meta(meta: &Metadata<'static>) -> Option<models::Location> {
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

impl<S> QuiverLayer<S> {
    fn skip(meta: &tracing::Metadata<'_>) -> bool {
        let target = meta.target();
        let target = target.split("::").next().unwrap_or(target);

        matches!(target, env!("CARGO_CRATE_NAME") | "quinn" | "quinn_proto")
    }
}

impl<S> Layer<S> for QuiverLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
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

        if Self::skip(span.metadata()) {
            return;
        }

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

        if Self::skip(span.metadata()) {
            return;
        }

        if let Some(builder) = extensions.get_mut::<SpanBuilder>() {
            values.record(&mut SpanAttributeVisitor(&mut builder.tags));
        }
    }

    fn on_follows_from(&self, id: &span::Id, follows: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();

        if Self::skip(span.metadata()) {
            return;
        }

        let follows_span = ctx.span(follows).expect("follow span not found");
        let follows_extensions = follows_span.extensions();

        if let Some(builder) = extensions.get_mut::<SpanBuilder>() {
            if let Some(follows_builder) = follows_extensions.get::<SpanBuilder>() {
                builder.follows.push(models::Reference {
                    ty: models::RefType::FollowsFrom,
                    trace_id: follows_builder.trace_id,
                    span_id: follows_builder.span_id,
                });
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if Self::skip(event.metadata()) {
            return;
        }

        if let Some(span) = ctx.lookup_current() {
            let mut extensions = span.extensions_mut();

            if Self::skip(span.metadata()) {
                return;
            }

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

        if Self::skip(span.metadata()) {
            return;
        }

        if let Some(timings) = extensions.get_mut::<Timings>() {
            let now = self.clock.now();
            timings.idle += now.duration_since(timings.last);
            timings.last = now;
        }
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();

        if Self::skip(span.metadata()) {
            return;
        }

        if let Some(timings) = extensions.get_mut::<Timings>() {
            let now = self.clock.now();
            timings.busy += now.duration_since(timings.last);
            timings.last = now;
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("span not found");
        let mut extensions = span.extensions_mut();

        if Self::skip(span.metadata()) {
            return;
        }

        let builder = extensions
            .remove::<SpanBuilder>()
            .expect("span builder extension missing")
            .finish();
        let timings = extensions
            .remove::<Timings>()
            .expect("timings extension missing");

        let connection = self.connection.clone();
        let resource = self.resource.clone();

        tokio::spawn(async move {
            let span = models::Span {
                trace_id: builder.trace_id,
                span_id: builder.span_id,
                operation_name: builder.name.into(),
                flags: 1,
                references: builder.parent.into_iter().chain(builder.follows).collect(),
                start: builder.start_time,
                duration: builder.end_time - builder.start_time,
                location: builder.location,
                timing: models::Timing {
                    busy: timings.busy,
                    idle: timings.idle,
                },
                thread: builder
                    .thread_id
                    .zip(builder.thread.name().map(ToOwned::to_owned))
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

            if let Err(e) = connection.send_span(span).await {
                error!(error = ?e, "failed to send span data");
            }
        });
    }
}

pub async fn layer<S>(
    cert_pem: impl Into<Cow<'static, str>>,
) -> Result<(QuiverLayer<S>, Handle), BuildLayerError> {
    builder().with_server_cert(cert_pem).build().await
}

pub struct Handle {
    conn: connection::Handle,
}

impl Handle {
    pub async fn shutdown(self, max_wait: std::time::Duration) {
        self.conn.shutdown(max_wait).await;
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

        let endpoint = connection::create_endpoint(cert_pem.as_bytes())?;
        let connection = connection::create_connection(&endpoint, self.addr, self.name).await?;

        let handle = connection::Handle::new(endpoint, connection);

        let layer = QuiverLayer {
            connection: handle.clone(),
            clock: self.clock.unwrap_or_else(Clock::new),
            resource: self.resource.unwrap_or_else(Resource::new),
            _inner: PhantomData,
        };

        let handle = Handle { conn: handle };

        Ok((layer, handle))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuildLayerError {
    #[error("the server certificate must be specified")]
    MissingCertificate,
    #[error("failed to connect to the server")]
    Connect(#[from] crate::connection::ConnectError),
}

#[must_use]
pub fn builder() -> Builder {
    Builder::default()
}

struct SpanAttributeVisitor<'a>(&'a mut Vec<models::Tag>);

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

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::U64(value),
        });
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::I128(value),
        });
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::U128(value),
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

    // TODO: record error
    // fn record_error(
    //     &mut self,
    //     field: &tracing::field::Field,
    //     value: &(dyn std::error::Error + 'static),
    // ) {
    //     self.record_debug(field, &tracing::field::DisplayValue(value))
    // }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push(models::Tag {
            key: field.name().into(),
            value: models::TagValue::String(format!("{value:?}").into()),
        });
    }
}
