use std::io::Result;

fn main() -> Result<()> {
    tonic_build::configure().build_client(false).compile(
        &[
            "../jaeger-idl/proto/api_v2/collector.proto",
            "../jaeger-idl/proto/api_v2/model.proto",
        ],
        &["external", "../jaeger-idl/proto/api_v2"],
    )
}
