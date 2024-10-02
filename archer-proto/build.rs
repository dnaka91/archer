use std::{env, io::Result, path::PathBuf};

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::create_dir_all(out_dir.join("opentelemetry"))?;

    tonic_build::configure()
        .build_client(false)
        .out_dir(out_dir.join("opentelemetry"))
        .compile_protos(
            &["../opentelemetry-proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
            &["../opentelemetry-proto"],
        )
}
