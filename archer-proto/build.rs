use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=../jaeger-idl/opentelemetry-proto/opentelemetry");
    println!("cargo:rerun-if-changed=../jaeger-idl/proto");

    prost_build::compile_protos(
        &[
            "../jaeger-idl/proto/api_v2/model.proto",
            "../jaeger-idl/proto/api_v3/query_service.proto",
        ],
        &[
            "external",
            "../jaeger-idl/proto/api_v2",
            "../jaeger-idl/proto/api_v3",
            "../jaeger-idl/opentelemetry-proto",
        ],
    )
}
