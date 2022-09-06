use std::num::{NonZeroU128, NonZeroU64};

use anyhow::{Context, Result};
use archer_proto::{jaeger::api_v2 as proto, prost_types};
use time::{Duration, OffsetDateTime};

use crate::models::{Log, Process, RefType, Reference, Span, SpanId, Tag, TagValue, TraceId};

pub fn span(span: proto::Span) -> Result<Span> {
    Ok(Span {
        trace_id: trace_id(span.trace_id),
        span_id: span_id(span.span_id),
        operation_name: span.operation_name,
        flags: span.flags,
        references: span.references.into_iter().map(reference).collect(),
        start: timestamp(span.start_time.unwrap_or_default())?,
        duration: duration(span.duration.unwrap_or_default()),
        tags: span.tags.into_iter().map(key_value).collect(),
        logs: span.logs.into_iter().map(log).collect::<Result<_>>()?,
        process: process(span.process.context("process field missing")?),
    })
}

fn trace_id(id: Vec<u8>) -> TraceId {
    let mut buf = [0; 16];
    buf.copy_from_slice(&id);

    NonZeroU128::new(u128::from_be_bytes(buf))
        .unwrap_or_else(rand::random)
        .into()
}

fn span_id(id: Vec<u8>) -> SpanId {
    let mut buf = [0; 8];
    buf.copy_from_slice(&id);

    NonZeroU64::new(u64::from_be_bytes(buf))
        .unwrap_or_else(rand::random)
        .into()
}

fn reference(span_ref: proto::SpanRef) -> Reference {
    Reference {
        ty: match span_ref.ref_type() {
            proto::SpanRefType::ChildOf => RefType::ChildOf,
            proto::SpanRefType::FollowsFrom => RefType::FollowsFrom,
        },
        trace_id: trace_id(span_ref.trace_id),
        span_id: span_id(span_ref.span_id),
    }
}

fn timestamp(timestamp: prost_types::Timestamp) -> Result<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(
        i128::from(timestamp.seconds) * 1_000_000_000 + i128::from(timestamp.nanos),
    )
    .map_err(Into::into)
}

fn duration(duration: prost_types::Duration) -> Duration {
    Duration::seconds(duration.seconds) + Duration::nanoseconds(duration.nanos.into())
}

fn key_value(kv: proto::KeyValue) -> Tag {
    let ty = kv.v_type();

    Tag {
        key: kv.key,
        value: match ty {
            proto::ValueType::String => TagValue::String(kv.v_str),
            proto::ValueType::Bool => TagValue::Bool(kv.v_bool),
            proto::ValueType::Int64 => TagValue::I64(kv.v_int64),
            proto::ValueType::Float64 => TagValue::F64(kv.v_float64),
            proto::ValueType::Binary => TagValue::Binary(kv.v_binary),
        },
    }
}

fn log(log: proto::Log) -> Result<Log> {
    Ok(Log {
        timestamp: timestamp(log.timestamp.unwrap_or_default())?,
        fields: log.fields.into_iter().map(key_value).collect(),
    })
}

fn process(process: proto::Process) -> Process {
    Process {
        service: process.service_name,
        tags: process.tags.into_iter().map(key_value).collect(),
    }
}
