//! Child binary: runs the Toasty adapter for one benchmark configuration.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let (config_path, out_path) = bench_core::child::parse_child_args()?;
    let cfg = bench_core::child::read_config(&config_path)?;
    let adapter = bench_toasty::connect(cfg.database_url.clone(), cfg.pool_size).await?;
    bench_core::child::run_and_write(adapter, &config_path, &out_path).await
}
