#![allow(clippy::derive_partial_eq_without_eq)]

pub use prost;
pub use prost_types;

pub mod jaeger {
    pub mod api_v2 {
        include!(concat!(env!("OUT_DIR"), "/jaeger.api_v2.rs"));
    }

    pub mod api_v3 {
        include!(concat!(env!("OUT_DIR"), "/jaeger.api_v3.rs"));
    }
}

pub mod opentelemetry {
    pub mod proto {
        pub mod common {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry.proto.common.v1.rs"
                ));
            }
        }

        pub mod resource {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry.proto.resource.v1.rs"
                ));
            }
        }

        pub mod trace {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/opentelemetry.proto.trace.v1.rs"));
            }
        }
    }
}