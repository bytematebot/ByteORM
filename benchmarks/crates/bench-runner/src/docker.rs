//! Postgres lifecycle through docker compose.

use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::paths;

pub const DEFAULT_URL: &str = "postgres://bench:bench@127.0.0.1:55432/bench";

fn compose(args: &[&str]) -> Result<()> {
    let status = Command::new("docker")
        .arg("compose")
        .args(["-f", root_compose().as_str()])
        .args(args)
        .status()
        .context("running docker compose (is Docker installed and running?)")?;
    if !status.success() {
        bail!("docker compose {:?} failed with {status}", args);
    }
    Ok(())
}

fn root_compose() -> String {
    paths::root()
        .join("docker-compose.yml")
        .to_string_lossy()
        .into_owned()
}

/// Start Postgres and wait for its healthcheck.
pub fn up() -> Result<()> {
    compose(&["up", "-d", "--wait"])
}

pub fn down() -> Result<()> {
    compose(&["down", "-v"])
}

/// True when the caller set `DATABASE_URL`, in which case the tool uses that
/// database and never touches Docker.
pub fn external_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok().filter(|u| !u.is_empty())
}
