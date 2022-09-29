use std::num::{NonZeroU128, NonZeroU64};

use anyhow::Result;
use archer_thrift::jaeger as thrift;
use time::{Duration, OffsetDateTime};

use crate::models::{Log, Process, RefType, Reference, Span, SpanId, Tag, TagValue, TraceId};

pub fn span(span: thrift::Span, process: thrift::Process) -> Result<Span> {
    let references = span.references.unwrap_or_default();

    let parent = parent_span_id(
        span.parent_span_id,
        (span.trace_id_high, span.trace_id_low),
        &references,
    );

    #[allow(clippy::cast_sign_loss)]
    Ok(Span {
        trace_id: trace_id(span.trace_id_high, span.trace_id_low),
        span_id: span_id(span.span_id),
        operation_name: span.operation_name,
        references: parent.into_iter().chain(references).map(span_ref).collect(),
        flags: span.flags as _,
        start: timestamp(span.start_time)?,
        duration: duration(span.duration),
        tags: span.tags.unwrap_or_default().into_iter().map(tag).collect(),
        logs: span
            .logs
            .unwrap_or_default()
            .into_iter()
            .map(log)
            .collect::<Result<_>>()?,
        process: self::process(process),
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
        ref_type: thrift::SpanRefType::ChildOf,
    })
}

#[allow(clippy::cast_sign_loss)]
fn trace_id(high: i64, low: i64) -> TraceId {
    NonZeroU128::new((u128::from(high as u64)) << 64 | u128::from(low as u64))
        .unwrap_or_else(rand::random)
        .into()
}

#[allow(clippy::cast_sign_loss)]
fn span_id(id: i64) -> SpanId {
    NonZeroU64::new(id as _).unwrap_or_else(rand::random).into()
}

fn span_ref(span_ref: thrift::SpanRef) -> Reference {
    Reference {
        ty: span_ref_type(span_ref.ref_type),
        trace_id: trace_id(span_ref.trace_id_high, span_ref.trace_id_low),
        span_id: span_id(span_ref.span_id),
    }
}

fn span_ref_type(ty: thrift::SpanRefType) -> RefType {
    match ty {
        thrift::SpanRefType::ChildOf => RefType::ChildOf,
        thrift::SpanRefType::FollowsFrom => RefType::FollowsFrom,
    }
}

fn log(log: thrift::Log) -> Result<Log> {
    Ok(Log {
        timestamp: timestamp(log.timestamp)?,
        fields: log.fields.into_iter().map(tag).collect(),
    })
}

fn timestamp(microseconds: i64) -> Result<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(microseconds) * 1000).map_err(Into::into)
}

fn duration(microseconds: i64) -> Duration {
    Duration::microseconds(microseconds)
}

fn process(process: thrift::Process) -> Process {
    Process {
        service: process.service_name,
        tags: process
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(tag)
            .collect(),
    }
}

fn tag(tag: thrift::Tag) -> Tag {
    match tag.v_type {
        thrift::TagType::Bool => Tag {
            key: tag.key,
            value: TagValue::Bool(tag.v_bool.unwrap_or_default()),
        },
        thrift::TagType::Binary => Tag {
            key: tag.key,
            value: TagValue::Binary(tag.v_binary.unwrap_or_default()),
        },
        thrift::TagType::Double => Tag {
            key: tag.key,
            value: TagValue::F64(tag.v_double.unwrap_or_default()),
        },
        thrift::TagType::Long => Tag {
            key: tag.key,
            value: TagValue::I64(tag.v_long.unwrap_or_default()),
        },
        thrift::TagType::String => Tag {
            key: tag.key,
            value: TagValue::String(tag.v_str.unwrap_or_default()),
        },
    }
}
