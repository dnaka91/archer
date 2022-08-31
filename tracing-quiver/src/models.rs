use std::{
    borrow::Cow,
    num::{NonZeroU128, NonZeroU64},
};

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Reference {
    pub ty: RefType,
    pub trace_id: NonZeroU128,
    pub span_id: NonZeroU64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RefType {
    ChildOf,
    FollowsFrom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag<'a> {
    pub key: Cow<'static, str>,
    pub value: TagValue<'a>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TagValue<'a> {
    String(Cow<'a, str>),
    Bool(bool),
    I64(i64),
    F64(f64),
    Binary(Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Log<'a> {
    pub timestamp: OffsetDateTime,
    pub fields: Vec<Tag<'a>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Process<'a> {
    pub service: Cow<'a, str>,
    pub tags: Vec<Tag<'a>>,
}
