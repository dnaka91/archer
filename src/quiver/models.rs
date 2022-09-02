use std::num::{NonZeroU128, NonZeroU64};

use serde::Deserialize;
use time::{Duration, OffsetDateTime};

#[derive(Debug, Deserialize)]
pub struct Span {
    pub trace_id: NonZeroU128,
    pub span_id: NonZeroU64,
    pub operation_name: String,
    pub flags: u32,
    pub references: Vec<Reference>,
    pub start: OffsetDateTime,
    pub duration: Duration,
    pub timing: Timing,
    pub location: Option<Location>,
    pub thread: Option<Thread>,
    pub tags: Vec<Tag>,
    pub logs: Vec<Log>,
    pub process: Process,
}

#[derive(Debug, Deserialize)]
pub struct Reference {
    pub ty: RefType,
    pub trace_id: NonZeroU128,
    pub span_id: NonZeroU64,
}

#[derive(Debug, Deserialize)]
pub enum RefType {
    ChildOf,
    FollowsFrom,
}

#[derive(Debug, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: TagValue,
}

#[derive(Debug, Deserialize)]
pub enum TagValue {
    String(String),
    Bool(bool),
    I64(i64),
    F64(f64),
    Binary(Vec<u8>),
}

#[derive(Debug, Deserialize)]
pub struct Log {
    pub timestamp: OffsetDateTime,
    pub level: LogLevel,
    pub target: String,
    pub location: Option<Location>,
    pub fields: Vec<Tag>,
}

#[derive(Debug, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Deserialize)]
pub struct Process {
    pub service: String,
    pub version: String,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Deserialize)]
pub struct Location {
    pub filepath: String,
    pub namespace: String,
    pub lineno: u32,
}

#[derive(Debug, Deserialize)]
pub struct Timing {
    pub busy: Duration,
    pub idle: Duration,
}

#[derive(Debug, Deserialize)]
pub struct Thread {
    pub id: u64,
    pub name: String,
}
