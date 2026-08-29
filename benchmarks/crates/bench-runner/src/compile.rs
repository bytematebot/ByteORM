//! Compile-time metrics.
//!
//! Two numbers per ORM: a cold `cargo build --release` of the adapter crate
//! with its own artifacts cleaned (dependency compile time included, because
//! that is what a user waits for on a fresh checkout), and a rebuild after the
//! model definitions are touched, which is what a user waits for on every
//! schema change. ByteORM additionally gets the wall time of `byteorm
//! generate`, the step the other ORMs do not have.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use bench_core::report::CompileResult;

use crate::paths;

pub struct CompileSpec {
    pub orm: &'static str,
    pub crate_name: &'static str,
    /// File touched to trigger the incremental rebuild.
    pub touch: &'static str,
    /// Adapters with their own `[workspace]` (Toasty) build from their own
    /// directory instead of `-p` inside the benchmark workspace.
    pub standalone: bool,
}

pub const SPECS: &[CompileSpec] = &[
    CompileSpec {
        orm: "byteorm",
        crate_name: "bench-byteorm",
        touch: "src/lib.rs",
        standalone: false,
    },
    CompileSpec {
        orm: "diesel-async",
        crate_name: "bench-diesel",
        touch: "src/schema.rs",
        standalone: false,
    },
    CompileSpec {
        orm: "sea-orm",
        crate_name: "bench-seaorm",
        touch: "src/entity.rs",
        standalone: false,
    },
    CompileSpec {
        orm: "sqlx",
        crate_name: "bench-sqlx",
        touch: "src/models.rs",
        standalone: false,
    },
    CompileSpec {
        orm: "toasty",
        crate_name: "bench-toasty",
        touch: "src/models.rs",
        standalone: true,
    },
    CompileSpec {
        orm: "raw (tokio-postgres)",
        crate_name: "bench-raw",
        touch: "src/lib.rs",
        standalone: false,
    },
];

fn cargo(args: &[&str], dir: &Path, target_dir: Option<&Path>) -> Result<()> {
    let mut command = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    command.args(args).current_dir(dir);
    if let Some(target) = target_dir {
        command.env("CARGO_TARGET_DIR", target);
    }
    let status = command
        .status()
        .with_context(|| format!("running cargo {args:?}"))?;
    if !status.success() {
        bail!("cargo {args:?} failed with {status}");
    }
    Ok(())
}

fn timed(args: &[&str], dir: &Path, target_dir: Option<&Path>) -> Result<f64> {
    let started = Instant::now();
    cargo(args, dir, target_dir)?;
    Ok(started.elapsed().as_secs_f64())
}

/// Compile time for one adapter crate.
///
/// The cold number is measured in a throwaway `CARGO_TARGET_DIR`, so it
/// includes every dependency and cannot be contaminated by artifacts another
/// ORM left behind - `cargo clean -p` leaves too much standing to be trusted
/// for this. The incremental number is a rebuild after the model definitions
/// are touched, which is what a developer waits for on every schema change.
pub fn measure(spec: &CompileSpec, cold: bool) -> Result<CompileResult> {
    let dir = if spec.standalone {
        paths::crate_dir(spec.crate_name)
    } else {
        paths::root()
    };
    let build: Vec<&str> = if spec.standalone {
        vec!["build", "--release"]
    } else {
        vec!["build", "--release", "-p", spec.crate_name]
    };
    let touched = paths::crate_dir(spec.crate_name).join(spec.touch);
    let mut result = CompileResult::default();

    if cold {
        let target = std::env::temp_dir().join(format!(
            "byteorm-bench-target-{}-{}",
            std::process::id(),
            spec.crate_name
        ));
        let _ = std::fs::remove_dir_all(&target);
        result.cold_build_s = Some(timed(&build, &dir, Some(&target))?);
        if touched.exists() {
            filetime_touch(&touched)?;
            result.incremental_build_s = Some(timed(&build, &dir, Some(&target))?);
        }
        // Cold builds cost gigabytes; do not leave them behind.
        let _ = std::fs::remove_dir_all(&target);
    } else if touched.exists() {
        // Make sure the crate is built before timing the rebuild, so the
        // number is a rebuild and not a first build.
        cargo(&build, &dir, None)?;
        filetime_touch(&touched)?;
        result.incremental_build_s = Some(timed(&build, &dir, None)?);
    }

    if spec.crate_name == "bench-byteorm" {
        result.codegen_s = Some(measure_codegen()?);
    }

    Ok(result)
}

/// `byteorm generate` wall time, with the CLI already built so only code
/// generation is timed.
pub fn measure_codegen() -> Result<f64> {
    let repo = paths::repo_root();
    let client_dir = paths::crate_dir("bench-byteorm");
    cargo(
        &["build", "--release", "-p", "byteorm", "--bin", "byteorm"],
        &repo,
        None,
    )?;

    let binary = repo.join("target/release/byteorm");
    let started = Instant::now();
    let status = Command::new(&binary)
        .arg("generate")
        .current_dir(&client_dir)
        .status()
        .with_context(|| format!("running {}", binary.display()))?;
    if !status.success() {
        bail!("byteorm generate failed with {status}");
    }
    Ok(started.elapsed().as_secs_f64())
}

/// Rewrite the file's mtime without changing its content.
fn filetime_touch(path: &Path) -> Result<()> {
    let content = std::fs::read(path)?;
    std::fs::write(path, content)?;
    Ok(())
}
