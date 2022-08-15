pub mod jaeger {
    pub mod api_v3 {
        include!(concat!(env!("OUT_DIR"), "/jaeger.api_v3.rs"));
    }
}

pub mod opentelemetry {
    pub mod proto {
        pub mod common {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry.proto.common.v1.rs"
                ));
            }
        }

        pub mod resource {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry.proto.resource.v1.rs"
                ));
            }
        }

        pub mod trace {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/opentelemetry.proto.trace.v1.rs"));
            }
        }
    }
}

pub mod http {
    use std::collections::HashMap;

    use serde::Serialize;

    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TraceId(String);

    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SpanId(String);

    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct ProcessId(String);

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Trace {
        #[serde(rename = "traceID")]
        pub trace_id: TraceId,
        pub spans: Vec<Span>,
        pub processes: HashMap<String, Process>,
        pub warnings: Vec<String>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Span {
        #[serde(rename = "traceID")]
        pub trace_id: TraceId,
        #[serde(rename = "spanID")]
        pub span_id: SpanId,
        // deprecated
        #[serde(rename = "parentSpanID", skip_serializing_if = "Option::is_none")]
        pub parent_span_id: Option<SpanId>,
        pub flags: u32,
        pub operation_name: String,
        pub references: Vec<Reference>,
        pub start_time: u64,
        pub duration: u64,
        pub tags: Vec<KeyValue>,
        pub logs: Vec<Log>,
        #[serde(rename = "processID")]
        pub process_id: ProcessId,
        pub process: Option<Process>,
        pub warnings: Vec<String>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Reference {
        pub ref_type: ReferenceType,
        #[serde(rename = "traceID")]
        pub trace_id: TraceId,
        #[serde(rename = "spanID")]
        pub span_id: SpanId,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum ReferenceType {
        ChildOf,
        FollowsFrom,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Process {
        pub service_name: String,
        pub tags: Vec<KeyValue>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Log {
        pub timestamp: u64,
        pub fields: Vec<KeyValue>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct KeyValue {
        pub key: String,
        #[serde(flatten)]
        pub value: Value,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "lowercase", tag = "type", content = "value")]
    pub enum Value {
        String(String),
        Bool(bool),
        Int64(i64),
        Float64(f64),
        Binary(Vec<u8>),
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DependencyLink {
        pub parent: String,
        pub child: String,
        pub call_count: u64,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Operation {
        pub name: String,
        pub span_kind: String,
    }
}
