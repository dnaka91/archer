use std::num::{NonZeroU128, NonZeroU64};

use anyhow::Result;
use archer_proto::opentelemetry::proto::{
    common::v1 as otlp_common, resource::v1 as otlp_res, trace::v1 as otlp,
};
use opentelemetry_semantic_conventions::resource;
use time::OffsetDateTime;

use crate::models::{Log, Process, RefType, Reference, Span, SpanId, Tag, TagValue, TraceId};

pub fn span_len(res_spans: &[otlp::ResourceSpans]) -> usize {
    res_spans
        .iter()
        .flat_map(|rs| &rs.scope_spans)
        .map(|ss| ss.spans.len())
        .sum()
}

pub fn span(res_spans: otlp::ResourceSpans) -> Result<Vec<Span>> {
    let resource = res_spans.resource.unwrap_or_default();
    let spans = res_spans.scope_spans;

    if resource.attributes.is_empty() && spans.is_empty() {
        return Ok(Vec::new());
    }

    let process = self::resource(resource);

    spans
        .into_iter()
        .flat_map(|ss| {
            ss.spans
                .into_iter()
                .map(move |s| (s, ss.scope.clone().unwrap_or_default()))
        })
        .map(|(span, lib_tags)| span2(span, lib_tags, process.clone()))
        .collect()
}

fn resource(mut resource: otlp_res::Resource) -> Process {
    if resource.attributes.is_empty() {
        return Process {
            service: "OTLPResourceNoServiceName".to_owned(),
            tags: Vec::new(),
        };
    }

    Process {
        service: find_service_name(&mut resource.attributes).unwrap_or_default(),
        tags: resource.attributes.into_iter().filter_map(tag).collect(),
    }
}

fn find_service_name(attributes: &mut [otlp_common::KeyValue]) -> Option<String> {
    use otlp_common::any_value::Value;

    let value = attributes
        .iter_mut()
        .find(|attr| attr.key == resource::SERVICE_NAME.as_str())
        .and_then(|attr| attr.value.take())
        .and_then(|value| value.value)?;

    match value {
        Value::StringValue(s) => Some(s),
        _ => None,
    }
}

fn tag(attribute: otlp_common::KeyValue) -> Option<Tag> {
    use otlp_common::any_value::Value;

    Some(Tag {
        key: attribute.key,
        value: match attribute.value?.value? {
            Value::StringValue(s) => TagValue::String(s),
            Value::BoolValue(b) => TagValue::Bool(b),
            Value::IntValue(i) => TagValue::I64(i),
            Value::DoubleValue(d) => TagValue::F64(d),
            Value::ArrayValue(a) => TagValue::String(
                serde_json::to_string(&a.values.into_iter().map(json_value).collect::<Vec<_>>())
                    .ok()?,
            ),
            Value::KvlistValue(l) => TagValue::String(
                serde_json::to_string(
                    &l.values
                        .into_iter()
                        .filter_map(|kv| kv.value.map(|v| (kv.key, json_value(v))))
                        .collect::<serde_json::Map<_, _>>(),
                )
                .ok()?,
            ),
            Value::BytesValue(b) => TagValue::String(base64::encode(b)),
        },
    })
}

fn json_value(any: otlp_common::AnyValue) -> serde_json::Value {
    use otlp_common::any_value::Value;

    let value = match any.value {
        Some(v) => v,
        None => return serde_json::Value::Null,
    };

    match value {
        Value::StringValue(s) => s.into(),
        Value::BoolValue(b) => b.into(),
        Value::IntValue(i) => i.into(),
        Value::DoubleValue(d) => d.into(),
        Value::ArrayValue(a) => a
            .values
            .into_iter()
            .map(json_value)
            .collect::<Vec<_>>()
            .into(),
        Value::KvlistValue(l) => l
            .values
            .into_iter()
            .filter_map(|kv| kv.value.map(|v| (kv.key, json_value(v))))
            .collect::<serde_json::Map<_, _>>()
            .into(),
        Value::BytesValue(b) => b.into(),
    }
}

fn span2(
    span: otlp::Span,
    lib_tags: otlp_common::InstrumentationScope,
    process: Process,
) -> Result<Span> {
    let trace_id = trace_id(&span.trace_id);
    let start = timestamp(span.start_time_unix_nano)?;
    let end = timestamp(span.end_time_unix_nano)?;
    let kind = span.kind();
    let status = span.status.unwrap_or_default();

    Ok(Span {
        trace_id,
        span_id: span_id(&span.span_id),
        operation_name: span.name,
        flags: 1,
        references: (span.parent_span_id.iter().copied().any(|v| v != 0))
            .then(|| Reference {
                ty: RefType::ChildOf,
                trace_id,
                span_id: span_id(&span.parent_span_id),
            })
            .into_iter()
            .chain(span.links.into_iter().map(link))
            .collect(),
        start,
        duration: end - start,
        tags: [
            tag_from_span_kind(kind),
            tag_from_status_code(status.code()),
            tag_from_error_status_code(status.code()),
            tag_from_status_message(status.message),
            tag_from_trace_state(span.trace_state),
        ]
        .into_iter()
        .chain(tags_from_inst_library(lib_tags))
        .chain(span.attributes.into_iter().map(tag))
        .flatten()
        .collect(),
        logs: span.events.into_iter().map(log).collect::<Result<_>>()?,
        process,
    })
}

fn trace_id(id: &[u8]) -> TraceId {
    let mut id = (id.len() == 16)
        .then(|| {
            let mut buf = [0; 16];
            buf.copy_from_slice(id);
            NonZeroU128::new(u128::from_be_bytes(buf))
        })
        .flatten();

    loop {
        match id {
            Some(id) => break id.into(),
            None => id = NonZeroU128::new(rand::random()),
        }
    }
}

fn span_id(id: &[u8]) -> SpanId {
    let mut id = (id.len() == 8)
        .then(|| {
            let mut buf = [0; 8];
            buf.copy_from_slice(id);
            NonZeroU64::new(u64::from_be_bytes(buf))
        })
        .flatten();

    loop {
        match id {
            Some(id) => break id.into(),
            None => id = NonZeroU64::new(rand::random()),
        }
    }
}

fn timestamp(timestamp: u64) -> Result<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(timestamp as _).map_err(Into::into)
}

fn link(link: otlp::span::Link) -> Reference {
    Reference {
        ty: RefType::FollowsFrom,
        trace_id: trace_id(&link.trace_id),
        span_id: span_id(&link.span_id),
    }
}

fn tag_from_span_kind(span_kind: otlp::span::SpanKind) -> Option<Tag> {
    use otlp::span::SpanKind;

    Some(Tag {
        key: "span.kind".to_owned(),
        value: TagValue::String(match span_kind {
            SpanKind::Unspecified => return None,
            SpanKind::Internal => "internal".to_owned(),
            SpanKind::Server => "server".to_owned(),
            SpanKind::Client => "client".to_owned(),
            SpanKind::Producer => "producer".to_owned(),
            SpanKind::Consumer => "consumer".to_owned(),
        }),
    })
}

fn tag_from_status_code(status_code: otlp::status::StatusCode) -> Option<Tag> {
    use otlp::status::StatusCode;

    Some(Tag {
        key: "otel.status_code".to_owned(),
        value: TagValue::String(match status_code {
            StatusCode::Unset => return None,
            StatusCode::Ok => "OK".to_owned(),
            StatusCode::Error => "ERROR".to_owned(),
        }),
    })
}

fn tag_from_error_status_code(status_code: otlp::status::StatusCode) -> Option<Tag> {
    use otlp::status::StatusCode;

    Some(Tag {
        key: "error".to_owned(),
        value: TagValue::Bool(match status_code {
            StatusCode::Error => true,
            _ => return None,
        }),
    })
}

fn tag_from_status_message(message: String) -> Option<Tag> {
    (!message.is_empty()).then(|| Tag {
        key: "otel.status_description".to_owned(),
        value: TagValue::String(message),
    })
}

fn tag_from_trace_state(trace_state: String) -> Option<Tag> {
    (!trace_state.is_empty()).then(|| Tag {
        key: "w3c.tracestate".to_owned(),
        value: TagValue::String(trace_state),
    })
}

fn tags_from_inst_library(inst_lib: otlp_common::InstrumentationScope) -> [Option<Tag>; 2] {
    [
        (!inst_lib.name.is_empty()).then(|| Tag {
            key: "otel.library.name".to_owned(),
            value: TagValue::String(inst_lib.name),
        }),
        (!inst_lib.version.is_empty()).then(|| Tag {
            key: "otel.library.version".to_owned(),
            value: TagValue::String(inst_lib.version),
        }),
    ]
}

fn log(event: otlp::span::Event) -> Result<Log> {
    Ok(Log {
        timestamp: timestamp(event.time_unix_nano)?,
        fields: (!event.name.is_empty())
            .then(|| Tag {
                key: "event".to_owned(),
                value: TagValue::String(event.name),
            })
            .into_iter()
            .chain(event.attributes.into_iter().filter_map(tag))
            .collect(),
    })
}
