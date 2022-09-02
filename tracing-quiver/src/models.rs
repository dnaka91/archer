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
    pub fields: Vec<Tag<'a>>,
}

#[derive(Debug, Serialize)]
pub struct Process<'a> {
    pub service: Cow<'a, str>,
    pub tags: Vec<Tag<'a>>,
}
