//! Baseline: hand-written SQL over `tokio-postgres` with a bb8 pool.
//!
//! Not an ORM. It exists to answer "what does this workload cost with no
//! abstraction at all", so every ORM's number can be read as overhead over
//! the driver rather than as an absolute.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use bench_core::{BenchResult, NewPost, OrmAdapter, PostRow, UserRow};
use tokio_postgres::NoTls;

type PgPool = Pool<PostgresConnectionManager<NoTls>>;

pub struct RawAdapter {
    pool: PgPool,
}

pub async fn connect(url: String, pool_size: u32) -> Result<Arc<dyn OrmAdapter>> {
    let manager = PostgresConnectionManager::new_from_stringlike(url.as_str(), NoTls)?;
    let pool = Pool::builder().max_size(pool_size).build(manager).await?;
    Ok(Arc::new(RawAdapter { pool }))
}

fn user_from_row(row: &tokio_postgres::Row) -> UserRow {
    UserRow {
        id: row.get(0),
        email: row.get(1),
        username: row.get(2),
        created_at: row.get(3),
    }
}

fn post_from_row(row: &tokio_postgres::Row) -> PostRow {
    PostRow {
        id: row.get(0),
        user_id: row.get(1),
        title: row.get(2),
        content: row.get(3),
        views: row.get(4),
        created_at: row.get(5),
    }
}

#[async_trait]
impl OrmAdapter for RawAdapter {
    fn name(&self) -> &'static str {
        "raw (tokio-postgres)"
    }

    fn version(&self) -> &'static str {
        "tokio-postgres 0.7"
    }

    async fn insert_user(&self, email: String, username: String) -> BenchResult<i32> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let row = conn
            .query_one(
                "INSERT INTO users (email, username) VALUES ($1, $2) \
                 RETURNING id, email, username, created_at",
                &[&email, &username],
            )
            .await?;
        Ok(row.get(0))
    }

    async fn insert_posts_many(&self, rows: Vec<NewPost>) -> BenchResult<u64> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let mut sql = String::from("INSERT INTO posts (user_id, title, content, views) VALUES ");
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                sql.push(',');
            }
            let base = i * 4;
            sql.push_str(&format!(
                "(${}, ${}, ${}, ${})",
                base + 1,
                base + 2,
                base + 3,
                base + 4
            ));
            params.push(&row.user_id);
            params.push(&row.title);
            params.push(&row.content);
            params.push(&row.views);
        }
        Ok(conn.execute(sql.as_str(), &params).await?)
    }

    async fn find_user_by_id(&self, id: i32) -> BenchResult<Option<UserRow>> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let row = conn
            .query_opt(
                "SELECT id, email, username, created_at FROM users WHERE id = $1",
                &[&id],
            )
            .await?;
        Ok(row.as_ref().map(user_from_row))
    }

    async fn find_user_by_email(&self, email: &str) -> BenchResult<Option<UserRow>> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let row = conn
            .query_opt(
                "SELECT id, email, username, created_at FROM users WHERE email = $1",
                &[&email],
            )
            .await?;
        Ok(row.as_ref().map(user_from_row))
    }

    async fn recent_posts(&self, user_id: i32, limit: i64) -> BenchResult<Vec<PostRow>> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let rows = conn
            .query(
                "SELECT id, user_id, title, content, views, created_at FROM posts \
                 WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
                &[&user_id, &limit],
            )
            .await?;
        Ok(rows.iter().map(post_from_row).collect())
    }

    async fn update_post_title(&self, post_id: i32, title: String) -> BenchResult<u64> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        Ok(conn
            .execute(
                "UPDATE posts SET title = $1 WHERE id = $2",
                &[&title, &post_id],
            )
            .await?)
    }

    async fn delete_post(&self, post_id: i32) -> BenchResult<u64> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        Ok(conn
            .execute("DELETE FROM posts WHERE id = $1", &[&post_id])
            .await?)
    }

    async fn count_posts(&self, user_id: i32) -> BenchResult<i64> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let row = conn
            .query_one("SELECT COUNT(*) FROM posts WHERE user_id = $1", &[&user_id])
            .await?;
        Ok(row.get(0))
    }

    async fn sum_views(&self, user_id: i32) -> BenchResult<i64> {
        let conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let row = conn
            .query_one(
                "SELECT COALESCE(SUM(views), 0)::BIGINT FROM posts WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.get(0))
    }

    async fn tx_user_with_posts(&self, email: String, posts: usize) -> BenchResult<i32> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let tx = conn.transaction().await?;
        let user_id: i32 = tx
            .query_one(
                "INSERT INTO users (email, username) VALUES ($1, $2) RETURNING id",
                &[&email, &"tx-user"],
            )
            .await?
            .get(0);
        for i in 0..posts {
            tx.execute(
                "INSERT INTO posts (user_id, title, content, views) VALUES ($1, $2, $3, $4)",
                &[
                    &user_id,
                    &format!("tx post {i}"),
                    &bench_core::scenario::BODY,
                    &(i as i32),
                ],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(user_id)
    }
}
