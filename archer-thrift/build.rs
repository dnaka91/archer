use std::path::PathBuf;

use walkdir::WalkDir;

fn main() {
    println!("cargo:rerun-if-changed=../jaeger-idl/thrift");

    let thrift = which::which("thrift").unwrap();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());

    let output = std::process::Command::new(thrift)
        .arg("-out")
        .arg(&out_dir)
        .args(["--gen", "rs", "-r"])
        .arg(root.join("../jaeger-idl/thrift/agent.thrift"))
        .output()
        .unwrap();

    if !output.status.success() {
        panic!(
            "failed running `thrift`: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for entry in WalkDir::new(&out_dir) {
        let entry = entry.unwrap();
        let ext = entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();

        if ext != "rs" {
            continue;
        }

        let content = std::fs::read_to_string(entry.path()).unwrap();
        let content = content
            .lines()
            .filter_map(|line| (!line.starts_with("#![")).then(|| format!("{line}\n")))
            .collect::<String>();

        std::fs::write(entry.path(), content).unwrap();
    }
}
