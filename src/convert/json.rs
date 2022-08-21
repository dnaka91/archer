use std::collections::HashMap;

use archer_http as json;
use time::{Duration, OffsetDateTime};

use crate::models::{Log, Process, RefType, Reference, Span, Tag, TagValue};

pub fn trace(trace_id: u128, spans: impl IntoIterator<Item = Span>) -> json::Trace {
    let mut processes = HashMap::new();
    let mut counter = 0;

    json::Trace {
        trace_id: trace_id.into(),
        spans: spans
            .into_iter()
            .map(|mut s| {
                let p = process(&mut processes, &mut counter, std::mem::take(&mut s.process));
                span(s, p)
            })
            .collect(),
        processes,
        warnings: vec![],
    }
}

fn span(span: Span, process_id: json::ProcessId) -> json::Span {
    json::Span {
        trace_id: span.trace_id.into(),
        span_id: span.span_id.into(),
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
        trace_id: span_ref.trace_id.into(),
        span_id: span_ref.span_id.into(),
    }
}

fn timestamp(timestamp: OffsetDateTime) -> u64 {
    (timestamp.unix_timestamp_nanos() / 1000) as _
}

fn duration(duration: Duration) -> u64 {
    duration.whole_microseconds() as _
}

fn key_value(kv: Tag) -> json::KeyValue {
    json::KeyValue {
        key: kv.key,
        value: match kv.value {
            TagValue::String(s) => json::Value::String(s),
            TagValue::Bool(b) => json::Value::Bool(b),
            TagValue::I64(i) => json::Value::Int64(i),
            TagValue::F64(f) => json::Value::Float64(f),
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
    processes: &mut HashMap<String, json::Process>,
    counter: &mut usize,
    process: Process,
) -> json::ProcessId {
    *counter += 1;
    let pid = format!("p{counter}");

    processes.insert(
        pid.clone(),
        json::Process {
            service_name: process.service,
            tags: process.tags.into_iter().map(key_value).collect(),
        },
    );

    pid.into()
}
