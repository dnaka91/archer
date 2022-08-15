use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &["jaeger-idl/proto/api_v3/query_service.proto"],
        &["jaeger-idl/proto/api_v3", "jaeger-idl/opentelemetry-proto"],
    )
}
