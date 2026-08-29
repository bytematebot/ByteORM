//! The contract every ORM adapter implements.
//!
//! Each method is one round trip against the shared schema, written the way a
//! normal user of that ORM would write it: idiomatic builders, no hand-rolled
//! SQL unless the ORM is a SQL-first library. An ORM that cannot express an
//! operation returns [`BenchError::Unsupported`] instead of faking it with raw
//! SQL - the report then shows `n/a` rather than a misleading number.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub enum BenchError {
    /// The ORM has no idiomatic API for this operation.
    Unsupported(&'static str),
    Other(anyhow::Error),
}

impl std::fmt::Display for BenchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchError::Unsupported(what) => write!(f, "unsupported: {what}"),
            BenchError::Other(e) => write!(f, "{e:#}"),
        }
    }
}

// Deliberately *not* `std::error::Error`: that would make `BenchError`
// convertible into `anyhow::Error`, which collides with the blanket `From`
// below that lets adapters use `?` on any ORM's own error type.
impl<E> From<E> for BenchError
where
    E: Into<anyhow::Error>,
{
    fn from(e: E) -> Self {
        BenchError::Other(e.into())
    }
}

pub type BenchResult<T> = Result<T, BenchError>;

#[derive(Debug, Clone, PartialEq)]
pub struct UserRow {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostRow {
    pub id: i32,
    pub user_id: i32,
    pub title: String,
    pub content: String,
    pub views: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewPost {
    pub user_id: i32,
    pub title: String,
    pub content: String,
    pub views: i32,
}

/// One ORM under test. Adapters are shared across concurrent tasks, so every
/// method takes `&self`; an ORM needing `&mut` (Toasty) clones its cheap
/// handle internally.
#[async_trait]
pub trait OrmAdapter: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    /// Version of the ORM crate under test, for the report header.
    fn version(&self) -> &'static str;

    async fn insert_user(&self, email: String, username: String) -> BenchResult<i32>;

    /// One statement inserting `rows.len()` posts, if the ORM offers batch
    /// insert; otherwise `Unsupported`.
    async fn insert_posts_many(&self, rows: Vec<NewPost>) -> BenchResult<u64>;

    async fn find_user_by_id(&self, id: i32) -> BenchResult<Option<UserRow>>;

    async fn find_user_by_email(&self, email: &str) -> BenchResult<Option<UserRow>>;

    /// `WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2`.
    async fn recent_posts(&self, user_id: i32, limit: i64) -> BenchResult<Vec<PostRow>>;

    async fn update_post_title(&self, post_id: i32, title: String) -> BenchResult<u64>;

    async fn delete_post(&self, post_id: i32) -> BenchResult<u64>;

    async fn count_posts(&self, user_id: i32) -> BenchResult<i64>;

    async fn sum_views(&self, user_id: i32) -> BenchResult<i64>;

    /// One transaction: create a user, then `posts` posts for it.
    async fn tx_user_with_posts(&self, email: String, posts: usize) -> BenchResult<i32>;
}
