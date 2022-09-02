use std::{
    borrow::Cow,
    num::{NonZeroU128, NonZeroU64},
};

use serde::Serialize;
use time::{Duration, OffsetDateTime};

#[derive(Debug, Serialize)]
pub struct Span<'a> {
    pub trace_id: NonZeroU128,
    pub span_id: NonZeroU64,
    pub operation_name: Cow<'a, str>,
    pub flags: u32,
    pub references: Vec<Reference>,
    pub start: OffsetDateTime,
    pub duration: Duration,
    pub timing: Timing,
    pub location: Option<Location<'a>>,
    pub thread: Option<Thread<'a>>,
    pub tags: Vec<Tag<'a>>,
    pub logs: Vec<Log<'a>>,
    pub process: Process<'a>,
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
pub struct Tag<'a> {
    pub key: Cow<'static, str>,
    pub value: TagValue<'a>,
}

#[derive(Debug, Serialize)]
pub enum TagValue<'a> {
    String(Cow<'a, str>),
    Bool(bool),
    I64(i64),
    F64(f64),
    Binary(Vec<u8>),
}

#[derive(Debug, Serialize)]
pub struct Log<'a> {
    pub timestamp: OffsetDateTime,
    pub level: LogLevel,
    pub target: Cow<'a, str>,
    pub location: Option<Location<'a>>,
    pub fields: Vec<Tag<'a>>,
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
pub struct Process<'a> {
    pub service: Cow<'a, str>,
    pub tags: Vec<Tag<'a>>,
}

#[derive(Debug, Serialize)]
pub struct Location<'a> {
    pub filepath: Cow<'a, str>,
    pub namespace: Cow<'a, str>,
    pub lineno: u32,
}

#[derive(Debug, Serialize)]
pub struct Timing {
    pub busy: Duration,
    pub idle: Duration,
}

#[derive(Debug, Serialize)]
pub struct Thread<'a> {
    pub id: u64,
    pub name: Cow<'a, str>,
}
