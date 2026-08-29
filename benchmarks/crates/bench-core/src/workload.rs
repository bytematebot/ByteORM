//! The measurement loop.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::adapter::{BenchError, OrmAdapter};
use crate::fixture::{self, FixtureSpec};
use crate::proc;
use crate::report::{OrmResult, ScenarioResult};
use crate::scenario::{OpCtx, Scenario, ScenarioConfig, run_op};
use crate::stats::Stats;

#[derive(Debug, Clone)]
pub struct RunConfig {
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

impl RunConfig {
    /// The delete scenario consumes one dedicated row per iteration, so the
    /// fixture must carry at least that many throwaway posts.
    fn disposable_posts(&self) -> usize {
        if self.scenarios.contains(&Scenario::DeleteOne) {
            self.iterations + self.warmup + 16
        } else {
            0
        }
    }

    pub(crate) fn fixture_spec(&self) -> FixtureSpec {
        FixtureSpec {
            users: self.fixture_users,
            posts_per_user: self.fixture_posts_per_user,
            disposable_posts: self.disposable_posts(),
            run_tag: self.run_tag.clone(),
        }
    }
}

/// Run every configured scenario against one ORM.
pub async fn run_orm(adapter: Arc<dyn OrmAdapter>, cfg: &RunConfig) -> Result<OrmResult> {
    let mut scenarios = Vec::with_capacity(cfg.scenarios.len());

    for scenario in &cfg.scenarios {
        // Reseed before every scenario: each one meets identical table sizes
        // and planner statistics, whatever the previous scenario wrote.
        let fixture = Arc::new(fixture::seed(&cfg.database_url, &cfg.fixture_spec()).await?);
        let ctx = Arc::new(OpCtx {
            fixture,
            config: cfg.scenario_config.clone(),
            seq: AtomicUsize::new(0),
            run_tag: cfg.run_tag.clone(),
        });

        scenarios.push(run_scenario(*scenario, &adapter, &ctx, cfg).await);
    }

    Ok(OrmResult {
        orm: adapter.name().to_string(),
        version: adapter.version().to_string(),
        scenarios,
        peak_rss_kb: proc::peak_rss_kb(),
        isolated_process: false,
    })
}

async fn run_scenario(
    scenario: Scenario,
    adapter: &Arc<dyn OrmAdapter>,
    ctx: &Arc<OpCtx>,
    cfg: &RunConfig,
) -> ScenarioResult {
    // Warm up connections, prepared-statement caches and the query planner.
    for _ in 0..cfg.warmup {
        if let Err(e) = run_op(scenario, adapter, ctx).await {
            return failed(scenario, e);
        }
    }

    let concurrency = cfg.concurrency.max(1);
    let per_task = cfg.iterations.div_ceil(concurrency);
    let started = Instant::now();

    let mut tasks = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let adapter = Arc::clone(adapter);
        let ctx = Arc::clone(ctx);
        tasks.push(tokio::spawn(async move {
            let mut samples = Vec::with_capacity(per_task);
            for _ in 0..per_task {
                let t0 = Instant::now();
                run_op(scenario, &adapter, &ctx).await?;
                samples.push(t0.elapsed());
            }
            Ok::<Vec<Duration>, BenchError>(samples)
        }));
    }

    let mut samples = Vec::with_capacity(per_task * concurrency);
    for task in tasks {
        match task.await {
            Ok(Ok(mut s)) => samples.append(&mut s),
            Ok(Err(e)) => return failed(scenario, e),
            Err(join) => {
                return ScenarioResult {
                    scenario: scenario.key().to_string(),
                    stats: None,
                    note: Some(format!("task panicked: {join}")),
                };
            }
        }
    }
    let wall = started.elapsed();

    ScenarioResult {
        scenario: scenario.key().to_string(),
        stats: Some(Stats::from_samples(samples, wall)),
        note: None,
    }
}

fn failed(scenario: Scenario, e: BenchError) -> ScenarioResult {
    let note = match e {
        BenchError::Unsupported(what) => format!("no idiomatic API: {what}"),
        BenchError::Other(e) => format!("error: {e:#}"),
    };
    ScenarioResult {
        scenario: scenario.key().to_string(),
        stats: None,
        note: Some(note),
    }
}

/// Drive the workload without recording anything.
///
/// Postgres, the page cache and the CPU all speed up over the first seconds of
/// a session, which showed up as a 2.8x advantage for whichever ORM ran last.
/// Every measured run is now preceded by this, so the database is already warm
/// when the first ORM is measured.
pub async fn warm_database(adapter: Arc<dyn OrmAdapter>, cfg: &RunConfig) -> Result<()> {
    let fixture = Arc::new(fixture::seed(&cfg.database_url, &cfg.fixture_spec()).await?);
    let ctx = Arc::new(OpCtx {
        fixture,
        config: cfg.scenario_config.clone(),
        seq: AtomicUsize::new(0),
        run_tag: format!("warm-{}", cfg.run_tag),
    });

    for scenario in &cfg.scenarios {
        for _ in 0..cfg.warmup {
            // A failure here is not a benchmark result; the measured run will
            // report it properly.
            if run_op(*scenario, &adapter, &ctx).await.is_err() {
                break;
            }
        }
    }
    Ok(())
}

/// Convenience wrapper used by the runner when it drives several ORMs inside
/// one process.
pub async fn run_all(
    adapters: Vec<Arc<dyn OrmAdapter>>,
    cfg: &RunConfig,
) -> Result<Vec<OrmResult>> {
    let mut out = Vec::with_capacity(adapters.len());
    for adapter in adapters {
        out.push(run_orm(adapter, cfg).await?);
    }
    Ok(out)
}
