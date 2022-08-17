use std::{hash::Hash, path::PathBuf};

use quote::quote;
use regex::{Captures, Regex};
use siphasher::sip128::{Hasher128, SipHasher13};
use walkdir::{DirEntry, WalkDir};

fn main() {
    let git = Regex::new(r"https://github.com/jaegertracing/jaeger-ui").unwrap();
    let jaeger = Regex::new(r"(?i)jaeger").unwrap();
    let sourcemap = Regex::new(r"\n/(\*|/)# sourceMappingURL=.+\.map( \*/)?").unwrap();

    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = root.join("jaeger-ui/packages/jaeger-ui/build");
    let walker = WalkDir::new(&root);

    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let out_assets = out.join("assets");

    let entries = walker
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|entry| {
            let entry = entry.unwrap();
            if entry.file_type().is_dir() || is_ignored(&entry) {
                return None;
            }

            println!("cargo:rerun-if-changed={}", entry.path().display());

            let (content, etag) = if is_textish(&entry) {
                let buf = std::fs::read_to_string(entry.path()).unwrap();
                let buf = git.replace_all(&buf, "https://github.com/dnaka91/archer");
                let buf = jaeger.replace_all(&buf, |caps: &Captures| {
                    match &caps[0] {
                        "jaeger" => "archer",
                        "Jaeger" => "Archer",
                        "JAEGER" => "ARCHER",
                        v => v,
                    }
                    .to_owned()
                });
                let buf = sourcemap.replace_all(&buf, "");

                let path = out_assets.join(entry.path().strip_prefix(&root).unwrap());

                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, buf.as_bytes()).unwrap();

                (
                    path.to_str().unwrap().to_owned(),
                    create_etag(buf.as_bytes()),
                )
            } else {
                let buf = std::fs::read(entry.path()).unwrap();
                (entry.path().to_str().unwrap().to_owned(), create_etag(&buf))
            };

            let route = format!(
                "/{}",
                entry.path().strip_prefix(&root).unwrap().to_str().unwrap()
            );
            let mime = mime_guess::from_path(entry.path())
                .first_or_octet_stream()
                .to_string();

            let content = quote! {
                #route => Asset {
                    content: include_bytes!(#content),
                    etag: #etag,
                    mime: #mime,
                }
            };

            Some(content)
        })
        .collect::<Vec<_>>();

    let code = quote! {
        pub struct Asset {
            pub content: &'static [u8],
            pub etag: &'static str,
            pub mime: &'static str,
        }

        static ASSETS: phf::Map<&'static str, Asset> = ::phf::phf_map! { #(#entries),* };
    };

    std::fs::write(out.join("assets.rs"), code.to_string()).unwrap();
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

fn is_textish(entry: &DirEntry) -> bool {
    let ext = entry
        .path()
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();

    entry.file_type().is_file() && ["js", "css", "html"].contains(&ext)
}

fn is_ignored(entry: &DirEntry) -> bool {
    entry
        .path()
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        == "map"
}

fn create_etag(data: &[u8]) -> String {
    let mut hasher = SipHasher13::new();
    data.hash(&mut hasher);

    format!("W/\"{:032x}\"", hasher.finish128().as_u128())
}
