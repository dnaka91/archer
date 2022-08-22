#![warn(clippy::unwrap_used)]

pub use json::trace as trace_to_json;
pub use otlp::span as span_from_otlp;
pub use proto::span as span_from_proto;
pub use thrift::span as span_from_thrift;

mod json;
mod otlp;
mod proto;
mod thrift;
