use std::{
    fmt::{self, Debug},
    num::{NonZeroU128, NonZeroU64},
};

use opentelemetry::{
    global, runtime,
    sdk::{
        self,
        export::trace::{ExportResult, SpanData, SpanExporter},
        trace::{self as sdktrace, Tracer, TracerProvider},
    },
    trace::{self, TraceError, TracerProvider as _},
    Array, KeyValue, Value,
};
use opentelemetry_semantic_conventions::resource;
use time::OffsetDateTime;

use crate::{
    models::{Log, Process, RefType, Reference, Span, SpanId, Tag, TagValue, TraceId},
    storage::Database,
};

pub fn install_batch(database: Database, config: sdktrace::Config) -> Tracer {
    let provider = TracerProvider::builder()
        .with_batch_exporter(OtlpSpanExporter(database), runtime::Tokio)
        .with_config(config)
        .build();

    let tracer = provider.versioned_tracer("archer-otlp", Some(env!("CARGO_PKG_VERSION")), None);
    let _ = global::set_tracer_provider(provider);

    tracer
}

struct OtlpSpanExporter(Database);

impl Debug for OtlpSpanExporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OtlpSpanExporter")
            .field(&"database")
            .finish()
    }
}

#[archer_http::axum::async_trait]
impl SpanExporter for OtlpSpanExporter {
    async fn export(&mut self, batch: Vec<SpanData>) -> ExportResult {
        let batch = batch.into_iter().map(convert_span).collect::<Vec<_>>();

        self.0
            .save_spans(batch)
            .await
            .map_err(|e| TraceError::Other(e.into()))
    }
}

fn convert_span(span: SpanData) -> Span {
    let trace_id = trace_id(span.span_context.trace_id());
    let start = OffsetDateTime::try_from(span.start_time).unwrap();
    let end = OffsetDateTime::try_from(span.end_time).unwrap();

    Span {
        trace_id,
        span_id: span_id(span.span_context.span_id()),
        operation_name: span.name.into_owned(),
        flags: span.span_context.trace_flags().to_u8() as _,
        references: (span.parent_span_id != trace::SpanId::INVALID)
            .then(|| Reference {
                ty: RefType::ChildOf,
                trace_id,
                span_id: span_id(span.parent_span_id),
            })
            .into_iter()
            .chain(span.links.into_iter().map(reference))
            .collect(),
        start,
        duration: end - start,
        tags: span
            .attributes
            .into_iter()
            .filter_map(|(key, value)| tag(KeyValue { key, value }))
            .collect(),
        logs: span
            .events
            .into_iter()
            .map(|event| Log {
                timestamp: event.timestamp.try_into().unwrap(),
                fields: (!event.name.is_empty())
                    .then(|| Tag {
                        key: "event".to_owned(),
                        value: TagValue::String(event.name.into_owned()),
                    })
                    .into_iter()
                    .chain(event.attributes.into_iter().filter_map(tag))
                    .collect(),
            })
            .collect(),
        process: process(span.resource.as_deref()),
    }
}

fn trace_id(id: trace::TraceId) -> TraceId {
    let mut id = NonZeroU128::new(u128::from_be_bytes(id.to_bytes()));

    loop {
        match id {
            Some(id) => break id.into(),
            None => id = NonZeroU128::new(rand::random()),
        }
    }
}

fn span_id(id: trace::SpanId) -> SpanId {
    let mut id = NonZeroU64::new(u64::from_be_bytes(id.to_bytes()));

    loop {
        match id {
            Some(id) => break id.into(),
            None => id = NonZeroU64::new(rand::random()),
        }
    }
}

fn reference(link: trace::Link) -> Reference {
    Reference {
        ty: RefType::FollowsFrom,
        trace_id: trace_id(link.span_context().trace_id()),
        span_id: span_id(link.span_context().span_id()),
    }
}

fn tag(kv: KeyValue) -> Option<Tag> {
    Some(Tag {
        key: kv.key.to_string(),
        value: match kv.value {
            Value::Bool(b) => TagValue::Bool(b),
            Value::I64(i) => TagValue::I64(i),
            Value::F64(f) => TagValue::F64(f),
            Value::String(s) => TagValue::String(s.into_owned()),
            Value::Array(a) => TagValue::String(match a {
                Array::Bool(b) => serde_json::to_string(&b).ok()?,
                Array::I64(i) => serde_json::to_string(&i).ok()?,
                Array::F64(f) => serde_json::to_string(&f).ok()?,
                Array::String(s) => serde_json::to_string(&s).ok()?,
            }),
        },
    })
}

fn process(resource: Option<&sdk::Resource>) -> Process {
    match resource {
        Some(res) => Process {
            service: res
                .iter()
                .find_map(|(key, value)| {
                    (key == &resource::SERVICE_NAME).then(|| value.as_str().into_owned())
                })
                .unwrap_or_default(),
            tags: res
                .into_iter()
                .filter_map(|(key, value)| {
                    (key != &resource::SERVICE_NAME)
                        .then(|| {
                            tag(KeyValue {
                                key: key.clone(),
                                value: value.clone(),
                            })
                        })
                        .flatten()
                })
                .collect(),
        },
        None => Process {
            service: "OTLPResourceNoServiceName".to_owned(),
            tags: Vec::new(),
        },
    }
}
