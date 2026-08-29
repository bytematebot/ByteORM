//! The table of ORMs under test.
//!
//! Most adapters live in this workspace and are linked into the runner. Toasty
//! is built as a separate binary (its dependency tree cannot share a Cargo
//! resolution graph with SQLx's), so it is driven through the same
//! child-process protocol the runner already uses for isolation.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use bench_core::OrmAdapter;

type Connect = fn(String, u32) -> Pin<Box<dyn Future<Output = Result<Arc<dyn OrmAdapter>>> + Send>>;

pub enum Kind {
    /// Linked into the runner; can be measured in-process or isolated.
    Linked(Connect),
    /// A separate crate with its own `[workspace]`, built on demand and run as
    /// a child process. `crate_dir` is relative to `benchmarks/`.
    External {
        crate_dir: &'static str,
        bin: &'static str,
    },
}

pub struct Entry {
    pub key: &'static str,
    pub kind: Kind,
}

macro_rules! linked {
    ($key:literal, $module:path) => {
        Entry {
            key: $key,
            kind: Kind::Linked(|url, pool| Box::pin($module(url, pool))),
        }
    };
}

pub fn all() -> Vec<Entry> {
    vec![
        linked!("byteorm", bench_byteorm::connect),
        linked!("diesel", bench_diesel::connect),
        linked!("seaorm", bench_seaorm::connect),
        linked!("sqlx", bench_sqlx::connect),
        Entry {
            key: "toasty",
            kind: Kind::External {
                crate_dir: "crates/bench-toasty",
                bin: "bench-toasty",
            },
        },
        linked!("raw", bench_raw::connect),
    ]
}

pub fn keys() -> Vec<&'static str> {
    all().into_iter().map(|e| e.key).collect()
}

pub fn find(key: &str) -> Option<Entry> {
    all().into_iter().find(|e| e.key == key)
}
