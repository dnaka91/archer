use anyhow::{bail, Result};
use archer_proto::{
    jaeger::api_v2::{KeyValue, Log, Process, Span, SpanRef, SpanRefType, ValueType},
    prost_types::{Duration, Timestamp},
};
use archer_thrift::jaeger as thrift;

pub fn span(span: thrift::Span, proc: Option<thrift::Process>) -> Result<Span> {
    let references = span.references.unwrap_or_default();

    let parent = parent_span_id(
        span.parent_span_id,
        (span.trace_id_high, span.trace_id_low),
        &references,
    );

    Ok(Span {
        trace_id: trace_id(span.trace_id_high, span.trace_id_low),
        span_id: span.span_id.to_be_bytes().to_vec(),
        operation_name: span.operation_name,
        references: parent
            .into_iter()
            .chain(references)
            .map(span_ref)
            .collect::<Result<_>>()?,
        flags: span.flags as _,
        start_time: Some(timestamp(span.start_time)),
        duration: Some(duration(span.duration)),
        tags: span
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(tag)
            .collect::<Result<_>>()?,
        logs: span
            .logs
            .unwrap_or_default()
            .into_iter()
            .map(log)
            .collect::<Result<_>>()?,
        process: proc.map(process).transpose()?,
        process_id: String::new(),
        warnings: Vec::new(),
    })
}

fn parent_span_id(
    span_id: i64,
    trace_id: (i64, i64),
    references: &[thrift::SpanRef],
) -> Option<thrift::SpanRef> {
    if span_id == 0
        || references.iter().any(|r| {
            r.span_id == span_id && r.trace_id_high == trace_id.0 && r.trace_id_low == trace_id.1
        })
    {
        return None;
    }

    Some(thrift::SpanRef {
        trace_id_high: trace_id.0,
        trace_id_low: trace_id.1,
        span_id,
        ref_type: thrift::SpanRefType::CHILD_OF,
    })
}

fn trace_id(high: i64, low: i64) -> Vec<u8> {
    let mut buf = vec![0u8; 16];
    (&mut buf[..8]).copy_from_slice(&high.to_be_bytes());
    (&mut buf[8..]).copy_from_slice(&low.to_be_bytes());

    buf
}

fn span_ref(span_ref: thrift::SpanRef) -> Result<SpanRef> {
    let ref_type = span_ref_type(span_ref.ref_type)?;

    Ok(SpanRef {
        trace_id: trace_id(span_ref.trace_id_high, span_ref.trace_id_low),
        span_id: span_ref.span_id.to_be_bytes().to_vec(),
        ref_type: ref_type as _,
    })
}

fn span_ref_type(ty: thrift::SpanRefType) -> Result<SpanRefType> {
    Ok(match ty {
        thrift::SpanRefType::CHILD_OF => SpanRefType::ChildOf,
        thrift::SpanRefType::FOLLOWS_FROM => SpanRefType::FollowsFrom,
        _ => bail!("invalid span reference type {ty:?}"),
    })
}

fn log(log: thrift::Log) -> Result<Log> {
    Ok(Log {
        timestamp: Some(timestamp(log.timestamp)),
        fields: log.fields.into_iter().map(tag).collect::<Result<_>>()?,
    })
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

fn process(process: thrift::Process) -> Result<Process> {
    Ok(Process {
        service_name: process.service_name,
        tags: process
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(tag)
            .collect::<Result<_>>()?,
    })
}

fn tag(tag: thrift::Tag) -> Result<KeyValue> {
    Ok(match tag.v_type {
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
        v => bail!("invalid tag type `{v:?}`"),
    })
}
