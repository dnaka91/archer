use crate::{
    models::{Log, Process, RefType, Reference, Span, Tag, TagValue},
    quiver::models as quiver,
};

pub fn span(span: quiver::Span) -> Span {
    Span {
        trace_id: span.trace_id.into(),
        span_id: span.span_id.into(),
        operation_name: span.operation_name,
        flags: span.flags,
        references: span
            .references
            .into_iter()
            .map(|reference| Reference {
                ty: match reference.ty {
                    quiver::RefType::ChildOf => RefType::ChildOf,
                    quiver::RefType::FollowsFrom => RefType::FollowsFrom,
                },
                trace_id: reference.trace_id.into(),
                span_id: reference.span_id.into(),
            })
            .collect(),
        start: span.start,
        duration: span.duration,
        tags: timing(span.timing)
            .into_iter()
            .chain(location(span.location).into_iter().flatten())
            .chain(thread(span.thread).into_iter().flatten())
            .chain(span.tags.into_iter().map(tag))
            .collect(),
        logs: span.logs.into_iter().map(log).collect(),
        process: process(span.process),
    }
}

fn tag(tag: quiver::Tag) -> Tag {
    Tag {
        key: tag.key,
        value: match tag.value {
            quiver::TagValue::String(s) => TagValue::String(s),
            quiver::TagValue::Bool(b) => TagValue::Bool(b),
            quiver::TagValue::I64(i) => TagValue::I64(i),
            quiver::TagValue::F64(f) => TagValue::F64(f),
            quiver::TagValue::Binary(b) => TagValue::Binary(b),
        },
    }
}

fn log(log: quiver::Log) -> Log {
    Log {
        timestamp: log.timestamp,
        fields: [
            Tag {
                key: "level".to_owned(),
                value: TagValue::String(
                    match log.level {
                        quiver::LogLevel::Trace => "TRACE",
                        quiver::LogLevel::Debug => "DEBUG",
                        quiver::LogLevel::Info => "INFO",
                        quiver::LogLevel::Warn => "WARN",
                        quiver::LogLevel::Error => "ERROR",
                    }
                    .to_owned(),
                ),
            },
            Tag {
                key: "target".to_owned(),
                value: TagValue::String(log.target),
            },
        ]
        .into_iter()
        .chain(location(log.location).into_iter().flatten())
        .chain(log.fields.into_iter().map(tag))
        .collect(),
    }
}

fn process(process: quiver::Process) -> Process {
    Process {
        service: process.service,
        tags: [Tag {
            key: "service.version".to_owned(),
            value: TagValue::String(process.version),
        }]
        .into_iter()
        .chain(process.tags.into_iter().map(tag))
        .collect(),
    }
}

fn timing(timing: quiver::Timing) -> [Tag; 2] {
    [
        Tag {
            key: "busy_ns".to_owned(),
            value: TagValue::I128(timing.busy.whole_nanoseconds()),
        },
        Tag {
            key: "idle_ns".to_owned(),
            value: TagValue::I128(timing.idle.whole_nanoseconds()),
        },
    ]
}

fn location(location: Option<quiver::Location>) -> Option<[Tag; 3]> {
    let location = location?;
    Some([
        Tag {
            key: "code.filepath".to_owned(),
            value: TagValue::String(location.filepath),
        },
        Tag {
            key: "code.namespace".to_owned(),
            value: TagValue::String(location.namespace),
        },
        Tag {
            key: "code.fileno".to_owned(),
            value: TagValue::I64(location.lineno.into()),
        },
    ])
}

fn thread(thread: Option<quiver::Thread>) -> Option<[Tag; 2]> {
    let thread = thread?;
    Some([
        Tag {
            key: "thread.id".to_owned(),
            value: TagValue::U64(thread.id),
        },
        Tag {
            key: "thread.name".to_owned(),
            value: TagValue::String(thread.name),
        },
    ])
}
