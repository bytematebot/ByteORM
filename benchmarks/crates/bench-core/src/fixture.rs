//! Database fixtures: schema creation and deterministic seed data.
//!
//! Seeding runs through raw `tokio_postgres`, never through an adapter, so no
//! ORM can influence the state the others are measured against.

use anyhow::{Context, Result};
use tokio_postgres::NoTls;

use crate::scenario::BODY;

pub const SCHEMA_SQL: &str = include_str!("../../../sql/schema.sql");

#[derive(Debug, Clone)]
pub struct FixtureSpec {
    pub users: usize,
    pub posts_per_user: usize,
    /// Extra posts reserved for `delete_one`, which consumes one per
    /// iteration.
    pub disposable_posts: usize,
    pub run_tag: String,
}

/// Ids and emails of the seeded rows. Indexing wraps, so a scenario may run
/// more iterations than there are fixture rows.
#[derive(Debug)]
pub struct Fixture {
    pub user_ids: Vec<i32>,
    pub user_emails: Vec<String>,
    pub post_ids: Vec<i32>,
    pub disposable_post_ids: Vec<i32>,
}

impl Fixture {
    pub fn user(&self, i: usize) -> i32 {
        self.user_ids[i % self.user_ids.len()]
    }

    pub fn user_email(&self, i: usize) -> String {
        self.user_emails[i % self.user_emails.len()].clone()
    }

    pub fn post(&self, i: usize) -> i32 {
        self.post_ids[i % self.post_ids.len()]
    }

    pub fn disposable_post(&self, i: usize) -> i32 {
        self.disposable_post_ids[i % self.disposable_post_ids.len()]
    }
}

pub async fn connect(url: &str) -> Result<tokio_postgres::Client> {
    let (client, connection) = tokio_postgres::connect(url, NoTls)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("fixture connection error: {e}");
        }
    });
    Ok(client)
}

/// Drop and recreate the benchmark tables.
pub async fn create_schema(url: &str) -> Result<()> {
    let client = connect(url).await?;
    client
        .batch_execute(SCHEMA_SQL)
        .await
        .context("applying sql/schema.sql")?;
    Ok(())
}

/// Truncate and reseed. Called before every scenario that writes, so each
/// adapter meets the same table sizes and the same planner statistics.
pub async fn seed(url: &str, spec: &FixtureSpec) -> Result<Fixture> {
    let client = connect(url).await?;
    client
        .batch_execute("TRUNCATE posts, users RESTART IDENTITY CASCADE")
        .await
        .context("truncating fixtures")?;

    let mut user_ids = Vec::with_capacity(spec.users);
    let mut user_emails = Vec::with_capacity(spec.users);

    // One multi-row INSERT per 1000 users keeps seeding fast even for large
    // fixtures without turning this helper into a benchmark of its own.
    for chunk_start in (0..spec.users).step_by(1000) {
        let chunk_end = (chunk_start + 1000).min(spec.users);
        let mut sql = String::from("INSERT INTO users (email, username) VALUES ");
        for i in chunk_start..chunk_end {
            if i > chunk_start {
                sql.push(',');
            }
            let email = format!("seed-{}-{i}@bench.local", spec.run_tag);
            sql.push_str(&format!("('{email}', 'user{i}')"));
            user_emails.push(email);
        }
        sql.push_str(" RETURNING id");
        for row in client.query(sql.as_str(), &[]).await? {
            user_ids.push(row.get::<_, i32>(0));
        }
    }

    let mut post_ids = Vec::new();
    for (idx, user_id) in user_ids.iter().enumerate() {
        if spec.posts_per_user == 0 {
            break;
        }
        let mut sql = String::from("INSERT INTO posts (user_id, title, content, views) VALUES ");
        for j in 0..spec.posts_per_user {
            if j > 0 {
                sql.push(',');
            }
            sql.push_str(&format!(
                "({user_id}, 'seed post {idx}/{j}', $1, {})",
                (j % 97) as i32
            ));
        }
        sql.push_str(" RETURNING id");
        for row in client.query(sql.as_str(), &[&BODY]).await? {
            post_ids.push(row.get::<_, i32>(0));
        }
    }

    let mut disposable_post_ids = Vec::with_capacity(spec.disposable_posts);
    let owner = *user_ids
        .first()
        .context("fixture needs at least one user")?;
    for chunk_start in (0..spec.disposable_posts).step_by(1000) {
        let chunk_end = (chunk_start + 1000).min(spec.disposable_posts);
        let mut sql = String::from("INSERT INTO posts (user_id, title, content, views) VALUES ");
        for i in chunk_start..chunk_end {
            if i > chunk_start {
                sql.push(',');
            }
            sql.push_str(&format!("({owner}, 'disposable {i}', $1, 0)"));
        }
        sql.push_str(" RETURNING id");
        for row in client.query(sql.as_str(), &[&BODY]).await? {
            disposable_post_ids.push(row.get::<_, i32>(0));
        }
    }

    client.batch_execute("ANALYZE users; ANALYZE posts").await?;

    Ok(Fixture {
        user_ids,
        user_emails,
        post_ids,
        disposable_post_ids,
    })
}
