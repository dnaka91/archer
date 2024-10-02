use miette::{IntoDiagnostic, Result};
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::create_dir_all(out_dir.join("opentelemetry")).into_diagnostic()?;

    let descriptor = protox::compile(
        ["../opentelemetry-proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
        ["../opentelemetry-proto"],
    )?;

    tonic_build::configure()
        .build_client(false)
        .out_dir(out_dir.join("opentelemetry"))
        .compile_fds(descriptor)
        .into_diagnostic()
}
