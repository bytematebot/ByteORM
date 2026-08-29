//! Filesystem layout of the benchmark workspace.

use std::path::{Path, PathBuf};

/// Root of the `benchmarks/` workspace, derived from this crate's manifest so
/// the tool works from any current directory.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/bench-runner lives two levels below the workspace root")
        .to_path_buf()
}

/// Root of the ByteORM repository itself, one level above `benchmarks/`.
pub fn repo_root() -> PathBuf {
    root()
        .parent()
        .expect("benchmarks/ has a parent")
        .to_path_buf()
}

pub fn crate_dir(name: &str) -> PathBuf {
    root().join("crates").join(name)
}

pub fn results_dir() -> PathBuf {
    root().join("results")
}
