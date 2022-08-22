use std::num::{NonZeroU128, NonZeroU64};

use anyhow::{bail, Context, Result};
use archer_thrift::jaeger as thrift;
use time::{Duration, OffsetDateTime};

use crate::models::{Log, Process, RefType, Reference, Span, SpanId, Tag, TagValue, TraceId};

pub fn span(span: thrift::Span, proc: Option<thrift::Process>) -> Result<Span> {
    let references = span.references.unwrap_or_default();

    let parent = parent_span_id(
        span.parent_span_id,
        (span.trace_id_high, span.trace_id_low),
        &references,
    );

    Ok(Span {
        trace_id: trace_id(span.trace_id_high, span.trace_id_low),
        span_id: span_id(span.span_id),
        operation_name: span.operation_name,
        references: parent
            .into_iter()
            .chain(references)
            .map(span_ref)
            .collect::<Result<_>>()?,
        flags: span.flags as _,
        start: timestamp(span.start_time)?,
        duration: duration(span.duration),
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
        process: process(proc.context("process field missing")?)?,
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

fn trace_id(high: i64, low: i64) -> TraceId {
    let mut id = NonZeroU128::new((high as u64 as u128) << 64 | low as u64 as u128);

    loop {
        match id {
            Some(id) => break id.into(),
            None => id = NonZeroU128::new(rand::random()),
        }
    }
}

fn span_id(id: i64) -> SpanId {
    let mut id = NonZeroU64::new(id as _);

    loop {
        match id {
            Some(id) => break id.into(),
            None => id = NonZeroU64::new(rand::random()),
        }
    }
}

fn span_ref(span_ref: thrift::SpanRef) -> Result<Reference> {
    Ok(Reference {
        ty: span_ref_type(span_ref.ref_type)?,
        trace_id: trace_id(span_ref.trace_id_high, span_ref.trace_id_low),
        span_id: span_id(span_ref.span_id),
    })
}

fn span_ref_type(ty: thrift::SpanRefType) -> Result<RefType> {
    Ok(match ty {
        thrift::SpanRefType::CHILD_OF => RefType::ChildOf,
        thrift::SpanRefType::FOLLOWS_FROM => RefType::FollowsFrom,
        _ => bail!("invalid span reference type {ty:?}"),
    })
}

fn log(log: thrift::Log) -> Result<Log> {
    Ok(Log {
        timestamp: timestamp(log.timestamp)?,
        fields: log.fields.into_iter().map(tag).collect::<Result<_>>()?,
    })
}

fn timestamp(microseconds: i64) -> Result<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(microseconds as i128 * 1000).map_err(Into::into)
}

fn duration(microseconds: i64) -> Duration {
    Duration::microseconds(microseconds)
}

fn process(process: thrift::Process) -> Result<Process> {
    Ok(Process {
        service: process.service_name,
        tags: process
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(tag)
            .collect::<Result<_>>()?,
    })
}

fn tag(tag: thrift::Tag) -> Result<Tag> {
    Ok(match tag.v_type {
        thrift::TagType::BOOL => Tag {
            key: tag.key,
            value: TagValue::Bool(tag.v_bool.unwrap_or_default()),
        },
        thrift::TagType::BINARY => Tag {
            key: tag.key,
            value: TagValue::Binary(tag.v_binary.unwrap_or_default()),
        },
        thrift::TagType::DOUBLE => Tag {
            key: tag.key,
            value: TagValue::F64(tag.v_double.unwrap_or_default().0),
        },
        thrift::TagType::LONG => Tag {
            key: tag.key,
            value: TagValue::I64(tag.v_long.unwrap_or_default()),
        },
        thrift::TagType::STRING => Tag {
            key: tag.key,
            value: TagValue::String(tag.v_str.unwrap_or_default()),
        },
        v => bail!("invalid tag type `{v:?}`"),
    })
}
