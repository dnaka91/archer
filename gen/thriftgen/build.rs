fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::process::Command::new("thrift")
        .arg("-out")
        .arg(out_dir)
        .args(["--gen", "rs", "-r", "../../jaeger-idl/thrift/jaeger.thrift"])
        .output()
        .unwrap();
}
