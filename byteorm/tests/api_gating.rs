//! Checks that the generated API only exposes an operation on the builders
//! that should have it. The mutation builders are all one generic type now,
//! so nothing but the trait bounds keeps `set_*` off a delete.
//!
//! The test generates a client, then compiles snippets against it: the ones
//! under `accepted` must build, the ones under `rejected` must not.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SCHEMA: &str = r#"
model Widget {
    id      BigInt  PrimaryKey
    name    String  NotNull
    count   Int     @default(0)
}
"#;

const ACCEPTED: &[(&str, &str)] = &[
    (
        "query_filters",
        "let _ = db.widget.find_many(|q| q.where_id(1).order_by_count_desc().limit(5));",
    ),
    (
        "create_values",
        "let _ = db.widget.create(|c| c.set_id(1).set_name(\"a\"));",
    ),
    (
        "create_where",
        "let _ = db.widget.create(|c| c.set_id(1).set_name(\"a\").where_name(\"a\".to_string()));",
    ),
    (
        "update_set_and_where",
        "let _ = db.widget.update(|u| u.where_id(1).set_name(\"b\").inc_count(1));",
    ),
    (
        "update_allow_all_rows",
        "let _ = db.widget.update(|u| u.set_name(\"b\").allow_all_rows());",
    ),
    (
        "delete_where",
        "let _ = db.widget.delete(|d| d.where_id(1));",
    ),
    (
        "upsert_pk_and_values",
        "let _ = db.widget.upsert(|u| u.where_id(1).set_name(\"c\").inc_count(2));",
    ),
];

const REJECTED: &[(&str, &str)] = &[
    (
        "delete_cannot_set",
        "let _ = db.widget.delete(|d| d.set_name(\"x\"));",
    ),
    (
        "delete_cannot_inc",
        "let _ = db.widget.delete(|d| d.inc_count(1));",
    ),
    (
        "create_cannot_inc",
        "let _ = db.widget.create(|c| c.set_id(1).set_name(\"a\").inc_count(1));",
    ),
    (
        "upsert_cannot_filter_non_pk",
        "let _ = db.widget.upsert(|u| u.where_name(\"x\".to_string()));",
    ),
    (
        "query_cannot_set",
        "let _ = db.widget.find_many(|q| q.set_name(\"x\"));",
    ),
    (
        "delete_cannot_allow_all_rows",
        "let _ = db.widget.delete(|d| d.allow_all_rows());",
    ),
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Builds the CLI once and generates a client from `SCHEMA`.
fn prepare_project(dir: &Path) {
    let root = workspace_root();

    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "byteorm", "--bin", "byteorm"])
        .current_dir(&root)
        .status()
        .expect("build the CLI");
    assert!(status.success(), "building the CLI failed");

    fs::create_dir_all(dir.join("src")).expect("create project");
    fs::write(dir.join("schema.bo"), SCHEMA).expect("write schema");
    fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "api-gating"
version = "0.0.0"
edition = "2024"

[workspace]

[dependencies]
byteorm-client = { path = "generated" }
tokio = { version = "1", features = ["full"] }
"#,
    )
    .expect("write manifest");
    fs::write(dir.join("src/lib.rs"), "").expect("write lib");

    let cli = root.join("target/debug/byteorm");
    let status = Command::new(cli)
        .arg("generate")
        .current_dir(dir)
        .status()
        .expect("run byteorm generate");
    assert!(status.success(), "generating the client failed");
}

/// Compiles one snippet and reports whether it built.
fn compiles(dir: &Path, snippet: &str) -> (bool, String) {
    fs::write(
        dir.join("src/lib.rs"),
        format!(
            r#"#![allow(unused)]
use byteorm_client::Client;

pub async fn probe(db: &Client) {{
    {snippet}
}}
"#
        ),
    )
    .expect("write snippet");

    let output = Command::new(env!("CARGO"))
        .args(["build", "--quiet"])
        .current_dir(dir)
        .output()
        .expect("compile snippet");

    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn builders_expose_only_the_operations_their_mode_allows() {
    let dir = workspace_root().join("target/api-gating");
    let _ = fs::remove_dir_all(&dir);
    prepare_project(&dir);

    let mut failures = Vec::new();

    for (name, snippet) in ACCEPTED {
        let (ok, stderr) = compiles(&dir, snippet);
        if !ok {
            failures.push(format!("{name} should compile but did not:\n{stderr}"));
        }
    }

    for (name, snippet) in REJECTED {
        let (ok, _) = compiles(&dir, snippet);
        if ok {
            failures.push(format!("{name} compiled but should have been rejected"));
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
