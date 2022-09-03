use std::{
    borrow::Cow,
    num::{NonZeroU128, NonZeroU64},
    sync::Arc,
};

use serde::Serialize;
use time::{Duration, OffsetDateTime};

#[derive(Debug, Serialize)]
pub struct Span {
    pub trace_id: NonZeroU128,
    pub span_id: NonZeroU64,
    pub operation_name: Cow<'static, str>,
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

#[derive(Debug, Serialize)]
pub struct Reference {
    pub ty: RefType,
    pub trace_id: NonZeroU128,
    pub span_id: NonZeroU64,
}

#[derive(Debug, Serialize)]
pub enum RefType {
    ChildOf,
    FollowsFrom,
}

#[derive(Debug, Serialize)]
pub struct Tag {
    pub key: Cow<'static, str>,
    pub value: TagValue,
}

#[derive(Debug, Serialize)]
pub enum TagValue {
    F64(f64),
    I64(i64),
    U64(u64),
    I128(i128),
    U128(u128),
    Bool(bool),
    String(Cow<'static, str>),
}

#[derive(Debug, Serialize)]
pub struct Log {
    pub timestamp: OffsetDateTime,
    pub level: LogLevel,
    pub target: Cow<'static, str>,
    pub location: Option<Location>,
    pub fields: Vec<Tag>,
}

#[derive(Debug, Serialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Serialize)]
pub struct Process {
    pub service: Arc<str>,
    pub version: Arc<str>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Serialize)]
pub struct Location {
    pub filepath: Cow<'static, str>,
    pub namespace: Cow<'static, str>,
    pub lineno: u32,
}

#[derive(Debug, Serialize)]
pub struct Timing {
    pub busy: Duration,
    pub idle: Duration,
}

#[derive(Debug, Serialize)]
pub struct Thread {
    pub id: u64,
    pub name: Cow<'static, str>,
}
