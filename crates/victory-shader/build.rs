//! Fingerprint the inputs to the shader translator for its disk cache.
//!
//! Only what decides the translation counts: the translator's sources
//! without its tests, and its own manifest and features. A cache holds
//! tens of thousands of translations, and an edited test, another crate's
//! dependency or a new compiler made every one of them anew. (The
//! translator is ordinary safe Rust over the container's bytes: the
//! compiler that built it does not change what it writes.)

use std::path::{Path, PathBuf};

const MANIFESTS: [&str; 2] = ["Cargo.toml", "build.rs"];

fn main() {
    println!("cargo:rerun-if-changed=src");
    for path in MANIFESTS {
        println!("cargo:rerun-if-changed={path}");
    }

    // Cargo runs build scripts in the crate directory. Hash only relative
    // file names so identical builds can share a cache across checkouts.
    let mut files: Vec<_> = MANIFESTS.into_iter().map(PathBuf::from).collect();
    sources(Path::new("src"), &mut files);
    let mut files: Vec<_> = files
        .into_iter()
        .map(|p| (p.to_string_lossy().replace('\\', "/"), p))
        .collect();
    files.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let mut hash = blake3::Hasher::new_derive_key("VIctory shader translator build identity v2");
    let mut features: Vec<_> = std::env::vars_os()
        .filter_map(|(name, _)| {
            let name = name.into_string().ok()?;
            name.starts_with("CARGO_FEATURE_").then_some(name)
        })
        .collect();
    features.sort_unstable();
    for feature in features {
        field(&mut hash, "feature", feature.as_bytes());
    }
    for (name, path) in files {
        let mut bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("read {name}: {error}"));
        // A checkout's line ends are no part of the source.
        bytes.retain(|&b| b != 13);
        field(&mut hash, &name, &bytes);
    }
    println!(
        "cargo:rustc-env=VICTORY_SHADER_SOURCE_HASH={}",
        hash.finalize().to_hex()
    );
}

fn field(hash: &mut blake3::Hasher, name: &str, value: &[u8]) {
    for bytes in [name.as_bytes(), value] {
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
}

fn sources(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    {
        let entry = entry.expect("read source entry");
        let path = entry.path();
        if entry.file_type().expect("read source file type").is_dir() {
            sources(&path, files);
        } else if !is_test(&path) {
            files.push(path);
        }
    }
}

/// Files compiled only into the crate's tests (`semantics_tests.rs`,
/// `lower/tests.rs`).
fn is_test(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "tests" || stem.ends_with("_tests"))
}
