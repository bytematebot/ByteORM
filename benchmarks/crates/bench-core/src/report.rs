//! Report model plus JSON and Markdown rendering.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::stats::Stats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario: String,
    pub stats: Option<Stats>,
    /// Set when the ORM cannot express the scenario, or when it failed.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrmResult {
    pub orm: String,
    pub version: String,
    pub scenarios: Vec<ScenarioResult>,
    /// Peak RSS of the process that ran this ORM, in kilobytes. Meaningful
    /// only when the runner isolates each ORM in its own process.
    pub peak_rss_kb: Option<u64>,
    pub isolated_process: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompileResult {
    /// Cold `cargo build --release` of the adapter crate, seconds.
    pub cold_build_s: Option<f64>,
    /// Rebuild after touching the model definitions, seconds.
    pub incremental_build_s: Option<f64>,
    /// Code generation step, for ORMs that have one (`byteorm generate`).
    pub codegen_s: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocResult {
    /// Non-blank, non-comment lines of the adapter's workload implementation.
    pub workload_loc: usize,
    /// Non-blank, non-comment lines of model/entity definitions, including
    /// generated code the user has to keep in their tree.
    pub model_loc: usize,
    pub generated_loc: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub started_at: String,
    pub database_url: String,
    pub iterations: usize,
    pub warmup: usize,
    pub concurrency: usize,
    pub pool_size: u32,
    pub batch_size: usize,
    pub page_size: i64,
    pub posts_per_transaction: usize,
    pub fixture_users: usize,
    pub fixture_posts_per_user: usize,
    pub host: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub meta: RunMeta,
    pub runtime: Vec<OrmResult>,
    #[serde(default)]
    pub compile: BTreeMap<String, CompileResult>,
    #[serde(default)]
    pub loc: BTreeMap<String, LocResult>,
}

impl Report {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serializes")
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# ByteORM benchmark report\n\n");
        out.push_str(&format!("Run: {}\n\n", self.meta.started_at));
        out.push_str("| setting | value |\n|---|---|\n");
        out.push_str(&format!("| iterations | {} |\n", self.meta.iterations));
        out.push_str(&format!("| warmup | {} |\n", self.meta.warmup));
        out.push_str(&format!("| concurrency | {} |\n", self.meta.concurrency));
        out.push_str(&format!("| pool size | {} |\n", self.meta.pool_size));
        out.push_str(&format!("| batch size | {} |\n", self.meta.batch_size));
        out.push_str(&format!("| page size | {} |\n", self.meta.page_size));
        out.push_str(&format!(
            "| posts per transaction | {} |\n",
            self.meta.posts_per_transaction
        ));
        out.push_str(&format!(
            "| fixture | {} users x {} posts |\n",
            self.meta.fixture_users, self.meta.fixture_posts_per_user
        ));
        for (k, v) in &self.meta.host {
            out.push_str(&format!("| {k} | {v} |\n"));
        }
        out.push('\n');

        out.push_str("## Runtime\n\n");
        out.push_str(&self.runtime_tables());

        if !self.compile.is_empty() {
            out.push_str("\n## Compile time\n\n");
            out.push_str(
                "| ORM | cold build (s) | incremental (s) | codegen (s) |\n|---|---:|---:|---:|\n",
            );
            for (orm, c) in &self.compile {
                out.push_str(&format!(
                    "| {orm} | {} | {} | {} |\n",
                    fmt_opt_s(c.cold_build_s),
                    fmt_opt_s(c.incremental_build_s),
                    fmt_opt_s(c.codegen_s),
                ));
            }
        }

        if !self.loc.is_empty() {
            out.push_str("\n## Ergonomics (lines of code, comments and blanks excluded)\n\n");
            out.push_str(
                "| ORM | workload | models | generated | total |\n|---|---:|---:|---:|---:|\n",
            );
            for (orm, l) in &self.loc {
                out.push_str(&format!(
                    "| {orm} | {} | {} | {} | {} |\n",
                    l.workload_loc,
                    l.model_loc,
                    l.generated_loc,
                    l.workload_loc + l.model_loc + l.generated_loc
                ));
            }
        }

        let memory: Vec<&OrmResult> = self
            .runtime
            .iter()
            .filter(|r| r.peak_rss_kb.is_some())
            .collect();
        if !memory.is_empty() {
            out.push_str("\n## Memory\n\n");
            out.push_str("| ORM | peak RSS (MiB) | isolated process |\n|---|---:|---|\n");
            for r in memory {
                out.push_str(&format!(
                    "| {} | {:.1} | {} |\n",
                    r.orm,
                    r.peak_rss_kb.unwrap_or(0) as f64 / 1024.0,
                    if r.isolated_process { "yes" } else { "no" }
                ));
            }
        }

        out
    }

    fn runtime_tables(&self) -> String {
        let mut out = String::new();
        let scenarios = self.scenario_keys();

        for scenario in scenarios {
            out.push_str(&format!("### `{scenario}`\n\n"));
            out.push_str(
                "| ORM | ops/s | mean (µs) | p50 | p95 | p99 | max | vs best |\n\
                 |---|---:|---:|---:|---:|---:|---:|---:|\n",
            );

            let mut rows: Vec<(&str, Option<&Stats>, Option<&str>)> = self
                .runtime
                .iter()
                .map(|orm| {
                    let sc = orm.scenarios.iter().find(|s| s.scenario == scenario);
                    (
                        orm.orm.as_str(),
                        sc.and_then(|s| s.stats.as_ref()),
                        sc.and_then(|s| s.note.as_deref()),
                    )
                })
                .collect();
            rows.sort_by(|a, b| match (a.1, b.1) {
                (Some(x), Some(y)) => y
                    .ops_per_sec
                    .partial_cmp(&x.ops_per_sec)
                    .unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });

            let best =
                rows.iter()
                    .filter_map(|r| r.1)
                    .map(|s| s.ops_per_sec)
                    .fold(
                        f64::NAN,
                        |acc, v| {
                            if acc.is_nan() || v > acc { v } else { acc }
                        },
                    );

            for (orm, stats, note) in rows {
                match stats {
                    Some(s) => out.push_str(&format!(
                        "| {orm} | {:.0} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.2}x |\n",
                        s.ops_per_sec,
                        s.mean_us,
                        s.p50_us,
                        s.p95_us,
                        s.p99_us,
                        s.max_us,
                        if best > 0.0 {
                            s.ops_per_sec / best
                        } else {
                            0.0
                        },
                    )),
                    None => out.push_str(&format!(
                        "| {orm} | n/a | | | | | | {} |\n",
                        note.unwrap_or("not supported")
                    )),
                }
            }
            out.push('\n');
        }
        out
    }

    fn scenario_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for orm in &self.runtime {
            for s in &orm.scenarios {
                if !keys.contains(&s.scenario) {
                    keys.push(s.scenario.clone());
                }
            }
        }
        keys
    }
}

fn fmt_opt_s(v: Option<f64>) -> String {
    match v {
        Some(v) => format!("{v:.2}"),
        None => "n/a".to_string(),
    }
}
