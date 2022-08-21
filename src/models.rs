use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

#[derive(Debug, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: u128,
    pub span_id: u64,
    pub operation_name: String,
    pub flags: u32,
    pub references: Vec<Reference>,
    pub start: OffsetDateTime,
    pub duration: Duration,
    pub tags: Vec<Tag>,
    pub logs: Vec<Log>,
    pub process: Process,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reference {
    pub ty: RefType,
    pub trace_id: u128,
    pub span_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RefType {
    ChildOf,
    FollowsFrom,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: TagValue,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TagValue {
    String(String),
    Bool(bool),
    I64(i64),
    F64(f64),
    Binary(Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Log {
    pub timestamp: OffsetDateTime,
    pub fields: Vec<Tag>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Process {
    pub service: String,
    pub tags: Vec<Tag>,
}
