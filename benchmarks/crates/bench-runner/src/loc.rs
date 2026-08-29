//! Ergonomics metric: how much code each ORM costs for the same workload.
//!
//! Counted per ORM:
//! * *workload* - the adapter implementation, the same ten operations in every
//!   crate, so the number is directly comparable.
//! * *models* - schema/entity/model definitions the developer writes by hand.
//! * *generated* - code a generator emits that lives in the developer's tree
//!   (ByteORM's client crate). Not written by hand, but shipped, reviewed and
//!   compiled, so it is reported separately rather than ignored.

use std::path::Path;

use anyhow::Result;
use bench_core::report::LocResult;

/// Non-blank lines that are not pure `//` comments.
pub fn count_file(path: &Path) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//") && !l.starts_with("#"))
        .count())
}

fn count_dir(dir: &Path) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    let mut total = 0;
    for entry in walk(dir)? {
        if entry.extension().is_some_and(|e| e == "rs") {
            total += count_file(&entry)?;
        }
    }
    Ok(total)
}

fn walk(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let path = entry?.path();
            // `target/` holds build output, not code anyone maintains.
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}

pub struct LocSpec {
    pub orm: &'static str,
    pub crate_name: &'static str,
    /// Files implementing the benchmark workload.
    pub workload: &'static [&'static str],
    /// Hand-written schema/model definitions.
    pub models: &'static [&'static str],
    /// Directories of generated-but-checked-in code.
    pub generated: &'static [&'static str],
}

pub const SPECS: &[LocSpec] = &[
    LocSpec {
        orm: "byteorm",
        crate_name: "bench-byteorm",
        workload: &["src/lib.rs"],
        models: &["schema.bo"],
        generated: &["generated/src"],
    },
    LocSpec {
        orm: "diesel-async",
        crate_name: "bench-diesel",
        workload: &["src/lib.rs"],
        models: &["src/schema.rs", "src/models.rs"],
        generated: &[],
    },
    LocSpec {
        orm: "sea-orm",
        crate_name: "bench-seaorm",
        workload: &["src/lib.rs"],
        models: &["src/entity.rs"],
        generated: &[],
    },
    LocSpec {
        orm: "sqlx",
        crate_name: "bench-sqlx",
        workload: &["src/lib.rs"],
        models: &["src/models.rs"],
        generated: &[],
    },
    LocSpec {
        orm: "toasty",
        crate_name: "bench-toasty",
        workload: &["src/lib.rs"],
        models: &["src/models.rs"],
        generated: &[],
    },
    LocSpec {
        orm: "raw (tokio-postgres)",
        crate_name: "bench-raw",
        workload: &["src/lib.rs"],
        models: &[],
        generated: &[],
    },
];

pub fn measure(spec: &LocSpec) -> Result<LocResult> {
    let dir = crate::paths::crate_dir(spec.crate_name);
    let mut result = LocResult::default();
    for f in spec.workload {
        result.workload_loc += count_file(&dir.join(f))?;
    }
    for f in spec.models {
        result.model_loc += count_file(&dir.join(f))?;
    }
    for d in spec.generated {
        result.generated_loc += count_dir(&dir.join(d))?;
    }
    Ok(result)
}
