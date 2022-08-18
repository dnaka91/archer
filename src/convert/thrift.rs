use archer_proto::{
    jaeger::api_v2::{KeyValue, Log, Process, Span, SpanRef, SpanRefType, ValueType},
    prost_types::{Duration, Timestamp},
};
use archer_thrift::jaeger as thrift;

pub fn span(span: thrift::Span, proc: Option<thrift::Process>) -> Span {
    // TODO: parent span ID handling

    Span {
        trace_id: trace_id(span.trace_id_high, span.trace_id_low),
        span_id: span.span_id.to_be_bytes().to_vec(),
        operation_name: span.operation_name,
        references: span
            .references
            .unwrap_or_default()
            .into_iter()
            .filter_map(span_ref)
            .collect(),
        flags: span.flags as _,
        start_time: Some(timestamp(span.start_time)),
        duration: Some(duration(span.duration)),
        tags: span.tags.unwrap_or_default().into_iter().map(tag).collect(),
        logs: span.logs.unwrap_or_default().into_iter().map(log).collect(),
        process: proc.map(process),
        process_id: String::new(),
        warnings: Vec::new(),
    }
}

fn trace_id(high: i64, low: i64) -> Vec<u8> {
    let mut buf = vec![0u8; 16];
    (&mut buf[..8]).copy_from_slice(&high.to_be_bytes());
    (&mut buf[8..]).copy_from_slice(&low.to_be_bytes());

    buf
}

fn span_ref(span_ref: thrift::SpanRef) -> Option<SpanRef> {
    Some(SpanRef {
        trace_id: trace_id(span_ref.trace_id_high, span_ref.trace_id_low),
        span_id: span_ref.span_id.to_be_bytes().to_vec(),
        ref_type: span_ref_type(span_ref.ref_type)? as _,
    })
}

fn span_ref_type(ty: thrift::SpanRefType) -> Option<SpanRefType> {
    Some(match ty {
        thrift::SpanRefType::CHILD_OF => SpanRefType::ChildOf,
        thrift::SpanRefType::FOLLOWS_FROM => SpanRefType::FollowsFrom,
        _ => return None,
    })
}

fn log(log: thrift::Log) -> Log {
    Log {
        timestamp: Some(timestamp(log.timestamp)),
        fields: log.fields.into_iter().map(tag).collect(),
    }
}

fn timestamp(microseconds: i64) -> Timestamp {
    let (seconds, nanos) = micros(microseconds);
    Timestamp { seconds, nanos }
}

fn duration(microseconds: i64) -> Duration {
    let (seconds, nanos) = micros(microseconds);
    Duration { seconds, nanos }
}

fn micros(micros: i64) -> (i64, i32) {
    let seconds = micros / 1_000_000;
    let nanos = 1000 * (micros % 1_000_000) as i32;

    (seconds, nanos)
}

fn process(process: thrift::Process) -> Process {
    Process {
        service_name: process.service_name,
        tags: process
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(tag)
            .collect(),
    }
}

fn tag(tag: thrift::Tag) -> KeyValue {
    match tag.v_type {
        thrift::TagType::BOOL => KeyValue {
            key: tag.key,
            v_type: ValueType::Bool as _,
            v_bool: tag.v_bool.unwrap_or_default(),
            ..KeyValue::default()
        },
        thrift::TagType::BINARY => KeyValue {
            key: tag.key,
            v_type: ValueType::Binary as _,
            v_binary: tag.v_binary.unwrap_or_default(),
            ..KeyValue::default()
        },
        thrift::TagType::DOUBLE => KeyValue {
            key: tag.key,
            v_type: ValueType::Float64 as _,
            v_float64: tag.v_double.unwrap_or_default().0,
            ..KeyValue::default()
        },
        thrift::TagType::LONG => KeyValue {
            key: tag.key,
            v_type: ValueType::Int64 as _,
            v_int64: tag.v_long.unwrap_or_default(),
            ..KeyValue::default()
        },
        thrift::TagType::STRING => KeyValue {
            key: tag.key,
            v_type: ValueType::String as _,
            v_str: tag.v_str.unwrap_or_default(),
            ..KeyValue::default()
        },
        v => KeyValue {
            key: tag.key,
            v_type: ValueType::Float64 as _,
            v_str: format!("unknown type `{v:?}`"),
            ..KeyValue::default()
        },
    }
}
