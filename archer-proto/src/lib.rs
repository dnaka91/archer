#![allow(clippy::derive_partial_eq_without_eq)]

pub use prost;
pub use prost_types;
pub use tonic;

pub mod jaeger {
    pub mod api_v2 {
        include!(concat!(env!("OUT_DIR"), "/jaeger/jaeger.api_v2.rs"));
    }
}

pub mod opentelemetry {
    pub mod proto {
        pub mod collector {
            pub mod trace {
                pub mod v1 {
                    include!(concat!(
                        env!("OUT_DIR"),
                        "/opentelemetry/opentelemetry.proto.collector.trace.v1.rs"
                    ));
                }
            }
        }

        pub mod common {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry/opentelemetry.proto.common.v1.rs"
                ));
            }
        }

        pub mod resource {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry/opentelemetry.proto.resource.v1.rs"
                ));
            }
        }

        pub mod trace {
            pub mod v1 {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/opentelemetry/opentelemetry.proto.trace.v1.rs"
                ));
            }
        }
    }
}
