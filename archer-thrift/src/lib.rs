// rules copied from the Thrift codegen output
#![allow(unused_imports)]
#![allow(unused_extern_crates)]
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::vec_box)]

mod models;

pub use models::{agent, jaeger};
pub use thrift;
use thrift::protocol::TInputProtocol;

trait ThriftDeserialize: Sized {
    fn read(prot: &mut impl TInputProtocol) -> thrift::Result<Self>;
}
