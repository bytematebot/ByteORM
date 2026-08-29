//! Cross-process run protocol.
//!
//! Every ORM is measured in its own process: peak RSS is then attributable to
//! one ORM, and an ORM whose dependency tree cannot coexist with another's in
//! a single Cargo workspace (Toasty and SQLx both pull a `libsqlite3-sys`, and
//! only one crate may link `sqlite3`) still runs under the same harness.
//!
//! The parent writes a [`ChildConfig`] as JSON, spawns the child, and reads
//! back an `OrmResult`.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::adapter::OrmAdapter;
use crate::report::OrmResult;
use crate::scenario::{Scenario, ScenarioConfig};
use crate::workload::RunConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildConfig {
    pub database_url: String,
    pub iterations: usize,
    pub warmup: usize,
    pub concurrency: usize,
    pub pool_size: u32,
    pub scenarios: Vec<Scenario>,
    pub scenario_config: ScenarioConfig,
    pub fixture_users: usize,
    pub fixture_posts_per_user: usize,
    pub run_tag: String,
}

impl From<&RunConfig> for ChildConfig {
    fn from(c: &RunConfig) -> Self {
        Self {
            database_url: c.database_url.clone(),
            iterations: c.iterations,
            warmup: c.warmup,
            concurrency: c.concurrency,
            pool_size: c.pool_size,
            scenarios: c.scenarios.clone(),
            scenario_config: c.scenario_config.clone(),
            fixture_users: c.fixture_users,
            fixture_posts_per_user: c.fixture_posts_per_user,
            run_tag: c.run_tag.clone(),
        }
    }
}

impl From<ChildConfig> for RunConfig {
    fn from(c: ChildConfig) -> Self {
        RunConfig {
            database_url: c.database_url,
            iterations: c.iterations,
            warmup: c.warmup,
            concurrency: c.concurrency,
            pool_size: c.pool_size,
            scenarios: c.scenarios,
            scenario_config: c.scenario_config,
            fixture_users: c.fixture_users,
            fixture_posts_per_user: c.fixture_posts_per_user,
            run_tag: c.run_tag,
        }
    }
}

pub fn read_config(path: impl AsRef<Path>) -> Result<RunConfig> {
    let text = std::fs::read_to_string(path)?;
    let config: ChildConfig = serde_json::from_str(&text)?;
    Ok(config.into())
}

pub fn write_result(path: impl AsRef<Path>, result: &OrmResult) -> Result<()> {
    std::fs::write(path, serde_json::to_string(result)?)?;
    Ok(())
}

/// Body of a child binary: run one adapter and write its result.
pub async fn run_and_write(
    adapter: Arc<dyn OrmAdapter>,
    config_path: impl AsRef<Path>,
    out_path: impl AsRef<Path>,
) -> Result<()> {
    let cfg = read_config(config_path)?;
    let result = crate::workload::run_orm(adapter, &cfg).await?;
    write_result(out_path, &result)
}

/// Parse the `--config` / `--out` pair every child binary accepts.
pub fn parse_child_args() -> Result<(String, String)> {
    let mut config = None;
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = args.next(),
            "--out" => out = args.next(),
            other => anyhow::bail!("unexpected argument {other:?}"),
        }
    }
    Ok((
        config.ok_or_else(|| anyhow::anyhow!("--config is required"))?,
        out.ok_or_else(|| anyhow::anyhow!("--out is required"))?,
    ))
}
