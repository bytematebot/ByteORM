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

/// Aggregate several runs of the same configuration.
///
/// One run of this suite carries a double-digit percentage of run-to-run
/// spread, so a single run cannot separate two close ORMs. These helpers take
/// the median across runs and refuse to call a lead a win when it is smaller
/// than the observed noise.
pub mod summary {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Serialize};

    use super::Report;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SummaryRow {
        pub orm: String,
        pub median_ops_per_sec: f64,
        /// Largest deviation from the median, as a fraction of it.
        pub spread: f64,
        /// Ratio to the fastest ORM in this scenario.
        pub vs_best: f64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ScenarioSummary {
        pub scenario: String,
        pub rows: Vec<SummaryRow>,
        /// Whether the leader's margin exceeds the noise of the top two.
        pub decisive: bool,
        pub verdict: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Summary {
        pub runs: usize,
        pub scenarios: Vec<ScenarioSummary>,
    }

    fn median(values: &mut [f64]) -> f64 {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = values.len() / 2;
        if values.len().is_multiple_of(2) {
            (values[mid - 1] + values[mid]) / 2.0
        } else {
            values[mid]
        }
    }

    fn spread(values: &[f64], median: f64) -> f64 {
        if values.len() < 2 || median == 0.0 {
            return 0.0;
        }
        values
            .iter()
            .map(|v| (v - median).abs() / median)
            .fold(0.0, f64::max)
    }

    pub fn summarize(reports: &[Report]) -> Summary {
        // scenario -> orm -> one throughput per run
        let mut collected: Vec<(String, BTreeMap<String, Vec<f64>>)> = Vec::new();

        for report in reports {
            for orm in &report.runtime {
                for scenario in &orm.scenarios {
                    let Some(stats) = &scenario.stats else {
                        continue;
                    };
                    let entry = match collected.iter_mut().find(|(k, _)| *k == scenario.scenario) {
                        Some((_, map)) => map,
                        None => {
                            collected.push((scenario.scenario.clone(), BTreeMap::new()));
                            &mut collected.last_mut().expect("just pushed").1
                        }
                    };
                    entry
                        .entry(orm.orm.clone())
                        .or_default()
                        .push(stats.ops_per_sec);
                }
            }
        }

        let scenarios = collected
            .into_iter()
            .map(|(scenario, per_orm)| {
                let mut rows: Vec<SummaryRow> = per_orm
                    .into_iter()
                    .map(|(orm, mut values)| {
                        let m = median(&mut values);
                        SummaryRow {
                            orm,
                            median_ops_per_sec: m,
                            spread: spread(&values, m),
                            vs_best: 0.0,
                        }
                    })
                    .collect();
                rows.sort_by(|a, b| {
                    b.median_ops_per_sec
                        .partial_cmp(&a.median_ops_per_sec)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let best = rows.first().map(|r| r.median_ops_per_sec).unwrap_or(0.0);
                for row in &mut rows {
                    row.vs_best = if best > 0.0 {
                        row.median_ops_per_sec / best
                    } else {
                        0.0
                    };
                }

                let (decisive, verdict) = match rows.as_slice() {
                    [first, second, ..] => {
                        let lead = first.median_ops_per_sec / second.median_ops_per_sec - 1.0;
                        let noise = first.spread.max(second.spread);
                        if lead > noise {
                            (
                                true,
                                format!(
                                    "{} leads {} by {:.0}%",
                                    first.orm,
                                    second.orm,
                                    lead * 100.0
                                ),
                            )
                        } else {
                            (
                                false,
                                format!(
                                    "tie: {} over {} by {:.0}%, inside the ±{:.0}% run-to-run noise",
                                    first.orm,
                                    second.orm,
                                    lead * 100.0,
                                    noise * 100.0
                                ),
                            )
                        }
                    }
                    _ => (false, "only one ORM measured".to_string()),
                };

                ScenarioSummary {
                    scenario,
                    rows,
                    decisive,
                    verdict,
                }
            })
            .collect();

        Summary {
            runs: reports.len(),
            scenarios,
        }
    }

    impl Summary {
        pub fn to_markdown(&self) -> String {
            let mut out = format!("# Median of {} runs\n\n", self.runs);
            out.push_str(
                "ORM order is rotated between runs and the database is warmed before each one, \
                 so position in the queue cannot decide the ranking.\n\n",
            );
            for scenario in &self.scenarios {
                out.push_str(&format!("## `{}`\n\n", scenario.scenario));
                out.push_str("| ORM | median ops/s | spread | vs best |\n|---|---:|---:|---:|\n");
                for row in &scenario.rows {
                    out.push_str(&format!(
                        "| {} | {:.0} | ±{:.0}% | {:.2}x |\n",
                        row.orm,
                        row.median_ops_per_sec,
                        row.spread * 100.0,
                        row.vs_best
                    ));
                }
                out.push_str(&format!("\n{}\n\n", scenario.verdict));
            }
            out
        }

        pub fn to_json(&self) -> String {
            serde_json::to_string_pretty(self).expect("summary serializes")
        }
    }
}
