pub use json::trace as trace_to_json;
pub use otlp::{span as span_from_otlp, span_len as span_from_otlp_len};
pub use quiver::span as span_from_quiver;

mod json;
mod otlp;
mod quiver;
