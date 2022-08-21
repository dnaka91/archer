#![allow(clippy::derive_partial_eq_without_eq)]

pub use prost;
pub use prost_types;
pub use tonic;

pub mod jaeger {
    pub mod api_v2 {
        include!(concat!(env!("OUT_DIR"), "/jaeger.api_v2.rs"));
    }
}
