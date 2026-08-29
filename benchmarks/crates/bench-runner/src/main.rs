//! `bench` - the ByteORM benchmark driver.
//!
//! Measures the same workload across ByteORM, Diesel, SeaORM, SQLx and Toasty,
//! plus a raw `tokio-postgres` baseline, on one Postgres instance and one
//! shared schema.

mod compile;
mod docker;
mod loc;
mod paths;
mod registry;

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use bench_core::child::ChildConfig;
use bench_core::report::{OrmResult, Report, RunMeta};
use bench_core::{RunConfig, Scenario, ScenarioConfig, fixture};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bench", about = "ByteORM comparative benchmark suite")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the benchmark Postgres container and apply the schema.
    Up,
    /// Stop the container and delete its volume.
    Down,
    /// Generate the ByteORM client and make sure the database is ready.
    Prepare,
    /// Run the runtime benchmark.
    Run(RunArgs),
    /// Measure compile time per ORM.
    Compile(CompileArgs),
    /// Count the code each ORM costs for the same workload.
    Loc,
    /// Run everything: runtime, compile time, lines of code.
    All(RunArgs),
    /// Internal: run one ORM inside a dedicated process.
    #[command(hide = true)]
    Child(ChildArgs),
}

#[derive(Args, Clone)]
struct RunArgs {
    /// ORMs to measure; defaults to all of them.
    #[arg(long = "orm", value_delimiter = ',')]
    orms: Vec<String>,
    /// Scenarios to measure; defaults to all of them.
    #[arg(long = "scenario", value_delimiter = ',')]
    scenarios: Vec<String>,
    #[arg(long, default_value_t = 2000)]
    iterations: usize,
    #[arg(long, default_value_t = 200)]
    warmup: usize,
    /// Concurrent tasks issuing the workload.
    #[arg(long, default_value_t = 1)]
    concurrency: usize,
    /// Connection pool size. Defaults to 20 because ByteORM's generated
    /// client hardcodes 20 and cannot be configured; matching it keeps the
    /// comparison fair at high concurrency.
    #[arg(long, default_value_t = 20)]
    pool_size: u32,
    #[arg(long, default_value_t = 100)]
    batch_size: usize,
    #[arg(long, default_value_t = 20)]
    page_size: i64,
    #[arg(long, default_value_t = 5)]
    posts_per_transaction: usize,
    #[arg(long, default_value_t = 200)]
    fixture_users: usize,
    #[arg(long, default_value_t = 20)]
    fixture_posts_per_user: usize,
    /// Run every ORM in its own process, so peak RSS is attributable.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    isolate: bool,
    /// Where to write report.json / report.md.
    #[arg(long)]
    out: Option<String>,
    /// Also run the cold-build measurement in `all` (slow: cleans the target
    /// directory once per ORM).
    #[arg(long, default_value_t = false)]
    cold_compile: bool,
}

#[derive(Args, Clone)]
struct ChildArgs {
    #[arg(long)]
    orm: String,
    #[arg(long)]
    config: String,
    #[arg(long)]
    out: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Up => {
            block_on(ensure_database())?;
            println!("database ready at {}", database_url());
            Ok(())
        }
        Command::Down => docker::down(),
        Command::Prepare => {
            generate_byteorm_client()?;
            block_on(ensure_database())?;
            println!("client generated, database ready at {}", database_url());
            Ok(())
        }
        Command::Run(args) => {
            let report = block_on(run_runtime(&args))?;
            emit(&report, args.out.as_deref())
        }
        Command::Compile(args) => {
            let mut report = empty_report();
            report.compile = run_compile(args.cold)?;
            emit(&report, args.out.as_deref())
        }
        Command::Loc => {
            let mut report = empty_report();
            report.loc = run_loc()?;
            emit(&report, None)
        }
        Command::All(args) => {
            let mut report = block_on(run_runtime(&args))?;
            report.compile = run_compile(args.cold_compile)?;
            report.loc = run_loc()?;
            emit(&report, args.out.as_deref())
        }
        Command::Child(args) => block_on(run_child(args)),
    }
}

#[derive(Args, Clone)]
struct CompileArgs {
    /// Clean the entire target directory before each ORM, so dependency
    /// compile time is included.
    #[arg(long, default_value_t = false)]
    cold: bool,
    #[arg(long)]
    out: Option<String>,
}

fn block_on<F: std::future::Future<Output = Result<T>>, T>(fut: F) -> Result<T> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(fut)
}

fn database_url() -> String {
    docker::external_url().unwrap_or_else(|| docker::DEFAULT_URL.to_string())
}

/// Start Postgres unless the caller pointed us at their own, then (re)create
/// the benchmark schema.
async fn ensure_database() -> Result<()> {
    if docker::external_url().is_none() {
        // Short, blocking, and only ever called from the top of a command.
        tokio::task::block_in_place(docker::up)?;
    }
    fixture::create_schema(&database_url()).await
}

fn generate_byteorm_client() -> Result<()> {
    let seconds = compile::measure_codegen()?;
    println!("byteorm generate: {seconds:.1}s");
    Ok(())
}

fn selected_orms(args: &RunArgs) -> Result<Vec<String>> {
    if args.orms.is_empty() {
        return Ok(registry::keys().into_iter().map(str::to_string).collect());
    }
    for orm in &args.orms {
        if registry::find(orm).is_none() {
            bail!("unknown orm {orm:?}; known: {:?}", registry::keys());
        }
    }
    Ok(args.orms.clone())
}

fn selected_scenarios(args: &RunArgs) -> Result<Vec<Scenario>> {
    if args.scenarios.is_empty() {
        return Ok(Scenario::ALL.to_vec());
    }
    args.scenarios
        .iter()
        .map(|s| {
            Scenario::parse(s).with_context(|| {
                format!(
                    "unknown scenario {s:?}; known: {:?}",
                    Scenario::ALL.iter().map(|s| s.key()).collect::<Vec<_>>()
                )
            })
        })
        .collect()
}

fn run_config(args: &RunArgs) -> Result<RunConfig> {
    Ok(RunConfig {
        database_url: database_url(),
        iterations: args.iterations,
        warmup: args.warmup,
        concurrency: args.concurrency,
        pool_size: args.pool_size,
        scenarios: selected_scenarios(args)?,
        scenario_config: ScenarioConfig {
            batch_size: args.batch_size,
            page_size: args.page_size,
            posts_per_transaction: args.posts_per_transaction,
        },
        fixture_users: args.fixture_users,
        fixture_posts_per_user: args.fixture_posts_per_user,
        run_tag: format!("{}", std::process::id()),
    })
}

async fn run_runtime(args: &RunArgs) -> Result<Report> {
    ensure_database().await?;
    let orms = selected_orms(args)?;
    let cfg = run_config(args)?;

    let mut results = Vec::new();
    for orm in &orms {
        eprintln!("==> {orm}");
        let entry = registry::find(orm).expect("checked above");
        let result = match (&entry.kind, args.isolate) {
            // An external adapter always runs as its own process; that is the
            // only way it can be linked at all.
            (registry::Kind::External { .. }, _) | (_, true) => run_isolated(orm, &cfg)?,
            (registry::Kind::Linked(connect), false) => {
                let adapter = connect(cfg.database_url.clone(), cfg.pool_size).await?;
                bench_core::workload::run_orm(adapter, &cfg).await?
            }
        };
        results.push(result);
    }

    Ok(Report {
        meta: meta(args, &cfg),
        runtime: results,
        compile: BTreeMap::new(),
        loc: BTreeMap::new(),
    })
}

/// Re-invoke this binary for a single ORM. Peak RSS is only meaningful when
/// one ORM's allocations are the only ones in the process.
fn run_isolated(orm: &str, cfg: &RunConfig) -> Result<OrmResult> {
    let dir = std::env::temp_dir().join(format!("byteorm-bench-{}-{orm}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let config_path = dir.join("config.json");
    let out_path = dir.join("result.json");
    std::fs::write(
        &config_path,
        serde_json::to_string(&ChildConfig::from(cfg))?,
    )?;

    let entry = registry::find(orm).with_context(|| format!("unknown orm {orm}"))?;
    let mut command = match entry.kind {
        registry::Kind::Linked(_) => {
            let mut c = std::process::Command::new(std::env::current_exe()?);
            c.arg("child").args(["--orm", orm]);
            c
        }
        registry::Kind::External { crate_dir, bin } => {
            std::process::Command::new(build_external(crate_dir, bin)?)
        }
    };
    let status = command
        .args(["--config", &config_path.to_string_lossy()])
        .args(["--out", &out_path.to_string_lossy()])
        .status()?;
    if !status.success() {
        bail!("child process for {orm} failed with {status}");
    }

    let mut result: OrmResult = serde_json::from_str(&std::fs::read_to_string(&out_path)?)?;
    result.isolated_process = true;
    Ok(result)
}

/// Build an adapter that lives in its own workspace and return its binary.
fn build_external(crate_dir: &str, bin: &str) -> Result<std::path::PathBuf> {
    let dir = paths::root().join(crate_dir);
    let status =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["build", "--release", "--bin", bin])
            .current_dir(&dir)
            .status()
            .with_context(|| format!("building {bin} in {}", dir.display()))?;
    if !status.success() {
        bail!("building {bin} failed with {status}");
    }
    Ok(dir.join("target/release").join(bin))
}

async fn run_child(args: ChildArgs) -> Result<()> {
    let cfg = bench_core::child::read_config(&args.config)?;
    let entry = registry::find(&args.orm).with_context(|| format!("unknown orm {}", args.orm))?;
    let registry::Kind::Linked(connect) = entry.kind else {
        bail!("{} runs as its own binary, not through `child`", args.orm);
    };
    let adapter: Arc<dyn bench_core::OrmAdapter> =
        connect(cfg.database_url.clone(), cfg.pool_size).await?;
    bench_core::child::run_and_write(adapter, &args.config, &args.out).await
}

fn run_compile(cold: bool) -> Result<BTreeMap<String, bench_core::report::CompileResult>> {
    let mut out = BTreeMap::new();
    for spec in compile::SPECS {
        eprintln!("==> compile {}", spec.orm);
        out.insert(spec.orm.to_string(), compile::measure(spec, cold)?);
    }
    Ok(out)
}

fn run_loc() -> Result<BTreeMap<String, bench_core::report::LocResult>> {
    let mut out = BTreeMap::new();
    for spec in loc::SPECS {
        out.insert(spec.orm.to_string(), loc::measure(spec)?);
    }
    Ok(out)
}

fn meta(args: &RunArgs, cfg: &RunConfig) -> RunMeta {
    let mut host = BTreeMap::new();
    host.insert("os".to_string(), std::env::consts::OS.to_string());
    host.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    if let Ok(cpus) = std::thread::available_parallelism() {
        host.insert("cpus".to_string(), cpus.to_string());
    }
    if let Ok(rustc) = std::process::Command::new("rustc")
        .arg("--version")
        .output()
    {
        host.insert(
            "rustc".to_string(),
            String::from_utf8_lossy(&rustc.stdout).trim().to_string(),
        );
    }

    RunMeta {
        started_at: chrono::Utc::now().to_rfc3339(),
        database_url: redact(&cfg.database_url),
        iterations: cfg.iterations,
        warmup: cfg.warmup,
        concurrency: cfg.concurrency,
        pool_size: cfg.pool_size,
        batch_size: args.batch_size,
        page_size: args.page_size,
        posts_per_transaction: args.posts_per_transaction,
        fixture_users: args.fixture_users,
        fixture_posts_per_user: args.fixture_posts_per_user,
        host,
    }
}

/// Reports are meant to be pasted into issues and READMEs, so credentials
/// never make it into one.
fn redact(url: &str) -> String {
    match (url.find("://"), url.find('@')) {
        (Some(scheme), Some(at)) if at > scheme => {
            format!("{}://***{}", &url[..scheme], &url[at..])
        }
        _ => url.to_string(),
    }
}

fn empty_report() -> Report {
    Report {
        meta: RunMeta {
            started_at: chrono::Utc::now().to_rfc3339(),
            database_url: String::new(),
            iterations: 0,
            warmup: 0,
            concurrency: 0,
            pool_size: 0,
            batch_size: 0,
            page_size: 0,
            posts_per_transaction: 0,
            fixture_users: 0,
            fixture_posts_per_user: 0,
            host: BTreeMap::new(),
        },
        runtime: Vec::new(),
        compile: BTreeMap::new(),
        loc: BTreeMap::new(),
    }
}

fn emit(report: &Report, out: Option<&str>) -> Result<()> {
    let markdown = report.to_markdown();
    println!("{markdown}");

    let dir = match out {
        Some(dir) => std::path::PathBuf::from(dir),
        None => paths::results_dir(),
    };
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("report.json"), report.to_json())?;
    std::fs::write(dir.join("report.md"), &markdown)?;
    eprintln!("wrote {}/report.json and report.md", dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn credentials_are_stripped_from_reported_urls() {
        assert_eq!(
            redact("postgres://bench:secret@127.0.0.1:55432/bench"),
            "postgres://***@127.0.0.1:55432/bench"
        );
        assert_eq!(redact("postgres:///bench"), "postgres:///bench");
    }
}
