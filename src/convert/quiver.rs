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
        tags: span.tags.into_iter().map(tag).collect(),
        logs: span
            .logs
            .into_iter()
            .map(|log| Log {
                timestamp: log.timestamp,
                fields: log.fields.into_iter().map(tag).collect(),
            })
            .collect(),
        process: Process {
            service: span.process.service,
            tags: span.process.tags.into_iter().map(tag).collect(),
        },
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
