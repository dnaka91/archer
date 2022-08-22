use std::{
    mem,
    num::{NonZeroU128, NonZeroU64},
};

use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

#[derive(Debug, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: TraceId,
    pub span_id: SpanId,
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
    pub trace_id: TraceId,
    pub span_id: SpanId,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RefType {
    ChildOf,
    FollowsFrom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: TagValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Process {
    pub service: String,
    pub tags: Vec<Tag>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TraceId(NonZeroU128);

impl TraceId {
    pub const fn get(self) -> NonZeroU128 {
        self.0
    }

    pub const fn to_bytes(self) -> [u8; mem::size_of::<Self>()] {
        self.0.get().to_be_bytes()
    }
}

impl From<NonZeroU128> for TraceId {
    fn from(value: NonZeroU128) -> Self {
        Self(value)
    }
}

impl TryFrom<&[u8]> for TraceId {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        const SIZE: usize = mem::size_of::<u128>();

        ensure!(
            value.len() == SIZE,
            "trace ID must be exactly {SIZE} bytes long"
        );

        let mut buf = [0; SIZE];
        buf.copy_from_slice(value);

        let value = u128::from_be_bytes(buf);

        Ok(Self(
            NonZeroU128::new(value).context("trace ID mustn't be empty")?,
        ))
    }
}

impl From<TraceId> for rusqlite::types::Value {
    fn from(id: TraceId) -> Self {
        id.to_bytes().to_vec().into()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpanId(NonZeroU64);

impl SpanId {
    pub const fn get(self) -> NonZeroU64 {
        self.0
    }

    pub const fn to_bytes(self) -> [u8; mem::size_of::<Self>()] {
        self.0.get().to_be_bytes()
    }
}

impl From<NonZeroU64> for SpanId {
    fn from(value: NonZeroU64) -> Self {
        Self(value)
    }
}

impl TryFrom<&[u8]> for SpanId {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        const SIZE: usize = mem::size_of::<u64>();

        ensure!(
            value.len() == SIZE,
            "span ID must be exactly {SIZE} bytes long"
        );

        let mut buf = [0; SIZE];
        buf.copy_from_slice(value);

        let value = u64::from_be_bytes(buf);

        Ok(Self(
            NonZeroU64::new(value).context("span ID mustn't be empty")?,
        ))
    }
}

impl From<SpanId> for rusqlite::types::Value {
    fn from(id: SpanId) -> Self {
        id.to_bytes().to_vec().into()
    }
}
