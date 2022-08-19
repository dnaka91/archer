use std::{collections::HashMap, time::UNIX_EPOCH};

use archer_http as json;
use archer_proto::{
    jaeger::api_v2::{KeyValue, Log, Process, Span, SpanRef, SpanRefType, ValueType},
    prost_types::{Duration, Timestamp},
};

pub fn trace(trace_id: Vec<u8>, spans: impl IntoIterator<Item = Span>) -> json::Trace {
    let mut processes = HashMap::new();
    let mut counter = 0;

    json::Trace {
        trace_id: trace_id.into(),
        spans: spans
            .into_iter()
            .map(|mut s| {
                let p = process(&mut processes, &mut counter, s.process.take().unwrap());
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
        start_time: timestamp(span.start_time.unwrap()),
        duration: duration(span.duration.unwrap()),
        tags: span.tags.into_iter().map(key_value).collect(),
        logs: span.logs.into_iter().map(log).collect(),
        process_id,
        process: None,
        warnings: Vec::new(),
    }
}

fn reference(span_ref: SpanRef) -> json::Reference {
    json::Reference {
        ref_type: match span_ref.ref_type() {
            SpanRefType::ChildOf => json::ReferenceType::ChildOf,
            SpanRefType::FollowsFrom => json::ReferenceType::FollowsFrom,
        },
        trace_id: span_ref.trace_id.into(),
        span_id: span_ref.span_id.into(),
    }
}

fn timestamp(timestamp: Timestamp) -> u64 {
    std::time::SystemTime::try_from(timestamp)
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn duration(duration: Duration) -> u64 {
    std::time::Duration::try_from(duration).unwrap().as_micros() as u64
}

fn key_value(kv: KeyValue) -> json::KeyValue {
    let v_type = kv.v_type();

    json::KeyValue {
        key: kv.key,
        value: match v_type {
            ValueType::String => json::Value::String(kv.v_str),
            ValueType::Bool => json::Value::Bool(kv.v_bool),
            ValueType::Int64 => json::Value::Int64(kv.v_int64),
            ValueType::Float64 => json::Value::Float64(kv.v_float64),
            ValueType::Binary => json::Value::Binary(kv.v_binary),
        },
    }
}

fn log(log: Log) -> json::Log {
    json::Log {
        timestamp: timestamp(log.timestamp.unwrap()),
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
            service_name: process.service_name,
            tags: process.tags.into_iter().map(key_value).collect(),
        },
    );

    pid.into()
}
