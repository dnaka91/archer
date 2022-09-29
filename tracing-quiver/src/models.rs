use std::{
    borrow::Cow,
    num::{NonZeroU128, NonZeroU64},
    sync::Arc,
};

use serde::Serialize;
use time::{Duration, OffsetDateTime};

/// Single, completed span event, that is part of a possibly larger trace. It may be the top-most
/// "root" span, marking the start of a trace, or have a parent peference, defining it as a child
/// of another _upper_ span.
#[derive(Debug, Serialize)]
pub struct Span {
    /// Unique (usually randomized) identifier for a single trace, that this span belongs to.
    pub trace_id: NonZeroU128,
    /// Unique (usually randomized) identifier for this very span.
    pub span_id: NonZeroU64,
    /// Name of this span.
    pub operation_name: Cow<'static, str>,
    /// Tracing flags, usually either `0` (means no flags) or `1`.
    pub flags: u32,
    /// List of references to other spans. Usually a single entry if this is a child span, or empty
    /// in case of a root span. Additionally, can include "follows from" references, which define
    /// non-child-parent relations to other spans.
    pub references: Vec<Reference>,
    /// Timestamp at which this span was created.
    pub start: OffsetDateTime,
    /// Lifetime of this span, being the duration from creation point until closed.
    pub duration: Duration,
    /// Additional timing information.
    pub timing: Timing,
    /// Optional source code location, where this span was created.
    pub location: Option<Location>,
    /// Optional information about the thread the span was created on.
    pub thread: Option<Thread>,
    /// Additional free-form tags, usually defined by the user.
    pub tags: Vec<Tag>,
    /// Log events that happened while this span was active.
    pub logs: Vec<Log>,
    /// Information about the application that creates and sends the traces.
    pub process: Process,
}

/// Single relation that defines an association between two spans.
#[derive(Debug, Serialize)]
pub struct Reference {
    /// The kind of reference.
    pub ty: RefType,
    /// Identifier of the trace that the target span belongs to. For [`RefType::ChildOf`]
    /// references, this is usually the same as the span that defines this reference.
    pub trace_id: NonZeroU128,
    /// Identifier of the targe span.
    pub span_id: NonZeroU64,
}

/// Type of [`Reference`] between tags.
#[derive(Debug, Serialize)]
pub enum RefType {
    /// The tag is a child of the target span, which is its parent (started before it, and stops
    /// after it).
    ChildOf,
    /// Non-child-parent association with the target span.
    /// TODO: Describe the use of this type in more detail.
    FollowsFrom,
}

/// A combination of a key and value, where the key is a textual label and the value is one of
/// several possible types.
#[derive(Debug, Serialize)]
pub struct Tag {
    /// Identifying name of this tag. Should be unique within a list.
    pub key: Cow<'static, str>,
    /// The tag's content.
    pub value: TagValue,
}

/// One of several possible types that describe a [`Tag`]'s value.
#[derive(Debug, Serialize)]
pub enum TagValue {
    // 64-bit floating point number.
    F64(f64),
    // 64-bit signed integer.
    I64(i64),
    // 64-bit unsigned integer.
    U64(u64),
    // 128-bit signed integer.
    I128(i128),
    // 128-bit unsigned integer.
    U128(u128),
    // Boolean `true/false` value.
    Bool(bool),
    // Free-form text.
    String(Cow<'static, str>),
}

/// Log event that happened during the (active) lifetime of a span.
#[derive(Debug, Serialize)]
pub struct Log {
    /// Point in time, at which the log was created.
    pub timestamp: OffsetDateTime,
    /// Severity level of the log message.
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

/// Source code location, where a span was created.
#[derive(Debug, Serialize)]
pub struct Location {
    /// Path to the source file.
    pub filepath: Cow<'static, str>,
    /// Additional namespace, usually a full module name in Rust (from the crate top-level down to
    /// the specific location).
    pub namespace: Cow<'static, str>,
    /// Line number within the source file.
    pub lineno: u32,
}

/// Fine-grained timing information for a span. It describes the sum of durations, during which the
/// span was marked as active and inactive.
#[derive(Debug, Serialize)]
pub struct Timing {
    /// Amount of time the span was marked as active.
    pub busy: Duration,
    /// Total inactive time of the span.
    pub idle: Duration,
}

/// Information about the thread, where a span was created.
#[derive(Debug, Serialize)]
pub struct Thread {
    /// Process-local identifier for the thread.
    pub id: u64,
    /// Thread name, which can be some generic value, or some specific name, depending on the
    /// creator of the thread.
    pub name: Cow<'static, str>,
}
