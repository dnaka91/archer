use archer_http as json;
use bimap::BiHashMap;
use time::{Duration, OffsetDateTime};

use crate::models::{Log, Process, RefType, Reference, Span, Tag, TagValue, TraceId};

pub fn trace(trace_id: TraceId, spans: impl IntoIterator<Item = Span>) -> json::Trace {
    let mut processes = BiHashMap::new();
    let mut counter = 0;

    json::Trace {
        trace_id: trace_id.get().into(),
        spans: spans
            .into_iter()
            .map(|mut s| {
                let p = process(&mut processes, &mut counter, std::mem::take(&mut s.process));
                span(s, p)
            })
            .collect(),
        processes: processes.into_iter().collect(),
        warnings: vec![],
    }
}

fn span(span: Span, process_id: json::ProcessId) -> json::Span {
    json::Span {
        trace_id: span.trace_id.get().into(),
        span_id: span.span_id.get().into(),
        parent_span_id: None,
        flags: span.flags,
        operation_name: span.operation_name,
        references: span.references.into_iter().map(reference).collect(),
        start_time: timestamp(span.start),
        duration: duration(span.duration),
        tags: span.tags.into_iter().map(key_value).collect(),
        logs: span.logs.into_iter().map(log).collect(),
        process_id,
        process: None,
        warnings: Vec::new(),
    }
}

fn reference(span_ref: Reference) -> json::Reference {
    json::Reference {
        ref_type: match span_ref.ty {
            RefType::ChildOf => json::ReferenceType::ChildOf,
            RefType::FollowsFrom => json::ReferenceType::FollowsFrom,
        },
        trace_id: span_ref.trace_id.get().into(),
        span_id: span_ref.span_id.get().into(),
    }
}

fn timestamp(timestamp: OffsetDateTime) -> i128 {
    timestamp.unix_timestamp_nanos() / 1000
}

fn duration(duration: Duration) -> i128 {
    duration.whole_microseconds()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn key_value(kv: Tag) -> json::KeyValue {
    json::KeyValue {
        key: kv.key,
        value: match kv.value {
            TagValue::F64(f) => json::Value::Float64(f),
            TagValue::I64(i) => json::Value::Int64(i),
            TagValue::U64(u) => json::Value::Int64(u as _),
            TagValue::I128(i) => json::Value::Int64(i as _),
            TagValue::U128(u) => json::Value::Int64(u as _),
            TagValue::Bool(b) => json::Value::Bool(b),
            TagValue::String(s) => json::Value::String(s),
            TagValue::Binary(b) => json::Value::Binary(b),
        },
    }
}

fn log(log: Log) -> json::Log {
    json::Log {
        timestamp: timestamp(log.timestamp),
        fields: log.fields.into_iter().map(key_value).collect(),
    }
}

fn process(
    processes: &mut BiHashMap<String, json::Process>,
    counter: &mut usize,
    process: Process,
) -> json::ProcessId {
    let process = json::Process {
        service_name: process.service,
        tags: process.tags.into_iter().map(key_value).collect(),
    };

    processes
        .get_by_right(&process)
        .cloned()
        .unwrap_or_else(|| {
            *counter += 1;
            let pid = format!("p{counter}");

            processes.insert(pid.clone(), process);

            pid
        })
        .into()
}
