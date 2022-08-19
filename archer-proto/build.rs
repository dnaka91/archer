use std::io::Result;

fn main() -> Result<()> {
    tonic_build::configure().build_client(false).compile(
        &[
            "../jaeger-idl/proto/api_v2/collector.proto",
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
