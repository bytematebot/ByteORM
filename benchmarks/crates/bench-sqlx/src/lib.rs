//! SQLx adapter: SQL-first, rows mapped with `FromRow`.
//!
//! Queries are written as runtime `query_as` rather than the compile-time
//! checked `query_as!` macro, so the benchmark builds without a live database
//! and every ORM's compile-time number stays comparable.

pub mod models;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bench_core::{BenchResult, NewPost, OrmAdapter, PostRow, UserRow};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::models::{Post, User};

pub struct SqlxAdapter {
    pool: PgPool,
}

pub async fn connect(url: String, pool_size: u32) -> Result<Arc<dyn OrmAdapter>> {
    let pool = PgPoolOptions::new()
        .max_connections(pool_size)
        .connect(&url)
        .await?;
    Ok(Arc::new(SqlxAdapter { pool }))
}

#[async_trait]
impl OrmAdapter for SqlxAdapter {
    fn name(&self) -> &'static str {
        "sqlx"
    }

    fn version(&self) -> &'static str {
        "0.9"
    }

    async fn insert_user(&self, email: String, username: String) -> BenchResult<i32> {
        let id: (i32,) =
            sqlx::query_as("INSERT INTO users (email, username) VALUES ($1, $2) RETURNING id")
                .bind(&email)
                .bind(&username)
                .fetch_one(&self.pool)
                .await?;
        Ok(id.0)
    }

    async fn insert_posts_many(&self, rows: Vec<NewPost>) -> BenchResult<u64> {
        // The idiomatic SQLx batch insert: unnest the columns as arrays, one
        // statement, four bind parameters no matter how many rows.
        let user_ids: Vec<i32> = rows.iter().map(|r| r.user_id).collect();
        let titles: Vec<String> = rows.iter().map(|r| r.title.clone()).collect();
        let contents: Vec<String> = rows.iter().map(|r| r.content.clone()).collect();
        let views: Vec<i32> = rows.iter().map(|r| r.views).collect();

        let result = sqlx::query(
            "INSERT INTO posts (user_id, title, content, views) \
             SELECT * FROM UNNEST($1::int[], $2::text[], $3::text[], $4::int[])",
        )
        .bind(&user_ids)
        .bind(&titles)
        .bind(&contents)
        .bind(&views)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn find_user_by_id(&self, id: i32) -> BenchResult<Option<UserRow>> {
        let user: Option<User> =
            sqlx::query_as("SELECT id, email, username, created_at FROM users WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(user.map(Into::into))
    }

    async fn find_user_by_email(&self, email: &str) -> BenchResult<Option<UserRow>> {
        let user: Option<User> =
            sqlx::query_as("SELECT id, email, username, created_at FROM users WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await?;
        Ok(user.map(Into::into))
    }

    async fn recent_posts(&self, user_id: i32, limit: i64) -> BenchResult<Vec<PostRow>> {
        let posts: Vec<Post> = sqlx::query_as(
            "SELECT id, user_id, title, content, views, created_at FROM posts \
             WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(posts.into_iter().map(Into::into).collect())
    }

    async fn update_post_title(&self, post_id: i32, title: String) -> BenchResult<u64> {
        let result = sqlx::query("UPDATE posts SET title = $1 WHERE id = $2")
            .bind(&title)
            .bind(post_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_post(&self, post_id: i32) -> BenchResult<u64> {
        let result = sqlx::query("DELETE FROM posts WHERE id = $1")
            .bind(post_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn count_posts(&self, user_id: i32) -> BenchResult<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM posts WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    async fn sum_views(&self, user_id: i32) -> BenchResult<i64> {
        let sum: (i64,) =
            sqlx::query_as("SELECT COALESCE(SUM(views), 0)::BIGINT FROM posts WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(sum.0)
    }

    async fn tx_user_with_posts(&self, email: String, posts: usize) -> BenchResult<i32> {
        let mut tx = self.pool.begin().await?;
        let user: (i32,) =
            sqlx::query_as("INSERT INTO users (email, username) VALUES ($1, $2) RETURNING id")
                .bind(&email)
                .bind("tx-user")
                .fetch_one(&mut *tx)
                .await?;
        for i in 0..posts {
            sqlx::query(
                "INSERT INTO posts (user_id, title, content, views) VALUES ($1, $2, $3, $4)",
            )
            .bind(user.0)
            .bind(format!("tx post {i}"))
            .bind(bench_core::scenario::BODY)
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(user.0)
    }
}
