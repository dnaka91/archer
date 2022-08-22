#![allow(dead_code)]

mod serde;

use std::{
    borrow::Cow,
    collections::HashMap,
    num::{NonZeroU128, NonZeroU64, ParseIntError},
    str::FromStr,
};

use ::serde::{Deserialize, Serialize};
pub use axum;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
pub use tower;
pub use tower_http;

pub enum ApiResponse<T> {
    Data(Vec<T>),
    Error(ApiError),
}

impl<T> Serialize for ApiResponse<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        #[derive(Serialize)]
        struct Response<'a, T> {
            data: &'a [T],
            total: usize,
            limit: usize,
            offset: usize,
            #[serde(skip_serializing_if = "Option::is_none")]
            errors: Option<[ResponseError<'a>; 1]>,
        }

        #[derive(Serialize)]
        struct ResponseError<'a> {
            code: u16,
            msg: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            trace_id: Option<&'a TraceId>,
        }

        let resp = match self {
            Self::Data(data) => Response {
                data,
                total: data.len(),
                limit: 0,
                offset: 0,
                errors: None,
            },
            Self::Error(error) => Response {
                data: &[],
                total: 0,
                limit: 0,
                offset: 0,
                errors: Some([ResponseError {
                    code: error.code.as_u16(),
                    msg: &error.msg,
                    trace_id: error.trace_id.as_ref(),
                }]),
            },
        };

        resp.serialize(serializer)
    }
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let code = if let Self::Error(ApiError { code, .. }) = &self {
            *code
        } else {
            StatusCode::OK
        };

        let mut resp = Json(self).into_response();
        if resp.status() == StatusCode::OK {
            *resp.status_mut() = code;
        }

        resp
    }
}

pub struct ApiError {
    pub code: StatusCode,
    pub msg: Cow<'static, str>,
    pub trace_id: Option<TraceId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TraceId(#[serde(with = "serde::hex")] pub u128);

impl From<u128> for TraceId {
    fn from(value: u128) -> Self {
        Self(value)
    }
}

impl From<NonZeroU128> for TraceId {
    fn from(value: NonZeroU128) -> Self {
        Self(value.get())
    }
}

impl FromStr for TraceId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u128::from_str_radix(s, 16).map(Self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpanId(#[serde(with = "serde::hex")] pub u64);

impl From<u64> for SpanId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<NonZeroU64> for SpanId {
    fn from(value: NonZeroU64) -> Self {
        Self(value.get())
    }
}

impl FromStr for SpanId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str_radix(s, 16).map(Self)
    }
}

#[derive(Serialize)]
#[serde(transparent)]
pub struct ProcessId(pub String);

impl From<String> for ProcessId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

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
