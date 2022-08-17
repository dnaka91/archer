// rules copied from the Thrift codegen output
#![allow(unused_imports)]
#![allow(unused_extern_crates)]
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::vec_box)]

pub use thrift;

pub mod agent {
    include!(concat!(env!("OUT_DIR"), "/agent.rs"));
}

pub mod jaeger {
    include!(concat!(env!("OUT_DIR"), "/jaeger.rs"));
}

pub mod zipkincore {
    include!(concat!(env!("OUT_DIR"), "/zipkincore.rs"));
}
