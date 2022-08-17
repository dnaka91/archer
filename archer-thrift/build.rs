fn main() {
    println!("cargo:rerun-if-changed=../jaeger-idl/thrift");

    let thrift = which::which("thrift").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    std::process::Command::new(thrift)
        .arg("-out")
        .arg(out_dir)
        .args([
            "--gen",
            "rs",
            "../jaeger-idl/thrift/agent.thrift",
            "../jaeger-idl/thrift/jaeger.thrift",
            "../jaeger-idl/thrift/zipkincore.thrift",
        ])
        .output()
        .unwrap();
}
