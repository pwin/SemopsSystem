//! Compiles every file under `resources/` into the crate, so the registry, the shapes, the
//! queries and the repair templates a binary runs are the ones it was built with, and a
//! command, a wheel and a WASM package built from one commit cannot disagree about them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let root =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets it")).join("resources");
    println!("cargo:rerun-if-changed=resources");
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();
    let mut out = String::from("&[\n");
    for (relative, absolute) in &files {
        writeln!(
            out,
            "    File {{ path: {relative:?}, text: include_str!({absolute:?}) }},"
        )
        .expect("writing to a String");
    }
    out.push_str("]\n");
    let target = PathBuf::from(env::var("OUT_DIR").expect("cargo sets it")).join("files.rs");
    fs::write(target, out).expect("writing the generated table");
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|e| e.expect("a directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, out);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("under the root")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((relative, path.to_string_lossy().into_owned()));
        }
    }
}
