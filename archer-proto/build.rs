use std::{env, io::Result, path::PathBuf};

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::create_dir_all(out_dir.join("jaeger"))?;
    std::fs::create_dir_all(out_dir.join("opentelemetry"))?;

    tonic_build::configure()
        .build_client(false)
        .out_dir(out_dir.join("jaeger"))
        .compile(
            &[
                "../jaeger-idl/proto/api_v2/collector.proto",
                "../jaeger-idl/proto/api_v2/model.proto",
            ],
            &["external", "../jaeger-idl/proto/api_v2"],
        )?;

    tonic_build::configure()
        .build_client(false)
        .out_dir(out_dir.join("opentelemetry"))
        .compile(
            &["../opentelemetry-proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
            &["../opentelemetry-proto"],
        )
}
