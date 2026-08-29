//! Toasty adapter.
//!
//! Toasty statements take `&mut dyn Executor`, and `Db` is a cheap clonable
//! handle over its own pool, so every operation clones the handle instead of
//! locking one shared connection - that keeps the concurrency shape the same
//! as the pool-based adapters.

pub mod models;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bench_core::{BenchError, BenchResult, NewPost, OrmAdapter, PostRow, UserRow};
use toasty::Db;

use crate::models::{Post, User};

pub struct ToastyAdapter {
    db: Db,
}

pub async fn connect(url: String, _pool_size: u32) -> Result<Arc<dyn OrmAdapter>> {
    let db = Db::builder()
        .models(toasty::models!(models::User, models::Post))
        .connect(&url)
        .await?;
    Ok(Arc::new(ToastyAdapter { db }))
}

fn user_row(u: &User) -> UserRow {
    UserRow {
        id: u.id,
        email: u.email.clone(),
        username: u.username.clone(),
        created_at: to_chrono(&u.created_at),
    }
}

fn post_row(p: &Post) -> PostRow {
    PostRow {
        id: p.id,
        user_id: p.user_id,
        title: p.title.clone(),
        content: p.content.clone(),
        views: p.views,
        created_at: to_chrono(&p.created_at),
    }
}

/// Toasty models timestamps with jiff; the harness compares rows across ORMs,
/// so they are normalized to `chrono` here.
fn to_chrono(ts: &toasty::stmt::Timestamp) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(ts.as_second(), ts.subsec_nanosecond() as u32)
        .unwrap_or_default()
}

#[async_trait]
impl OrmAdapter for ToastyAdapter {
    fn name(&self) -> &'static str {
        "toasty"
    }

    fn version(&self) -> &'static str {
        "0.10"
    }

    async fn insert_user(&self, email: String, username: String) -> BenchResult<i32> {
        let mut db = self.db.clone();
        let user = toasty::create!(User {
            email: email,
            username: username,
        })
        .exec(&mut db)
        .await
        .map_err(anyhow::Error::from)?;
        Ok(user.id)
    }

    async fn insert_posts_many(&self, rows: Vec<NewPost>) -> BenchResult<u64> {
        let mut db = self.db.clone();
        let count = rows.len() as u64;
        let mut stmt = Post::create_many();
        for row in rows {
            stmt = stmt.with_item(|c| {
                c.user_id(row.user_id)
                    .title(row.title)
                    .content(row.content)
                    .views(row.views)
            });
        }
        stmt.exec(&mut db).await.map_err(anyhow::Error::from)?;
        Ok(count)
    }

    async fn find_user_by_id(&self, id: i32) -> BenchResult<Option<UserRow>> {
        let mut db = self.db.clone();
        let user = User::filter_by_id(id)
            .first()
            .exec(&mut db)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(user.as_ref().map(user_row))
    }

    async fn find_user_by_email(&self, email: &str) -> BenchResult<Option<UserRow>> {
        let mut db = self.db.clone();
        let user = User::filter_by_email(email)
            .first()
            .exec(&mut db)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(user.as_ref().map(user_row))
    }

    async fn recent_posts(&self, user_id: i32, limit: i64) -> BenchResult<Vec<PostRow>> {
        let mut db = self.db.clone();
        let posts = Post::filter_by_user_id(user_id)
            .order_by(Post::fields().created_at().desc())
            .limit(limit as usize)
            .exec(&mut db)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(posts.iter().map(post_row).collect())
    }

    async fn update_post_title(&self, post_id: i32, title: String) -> BenchResult<u64> {
        let mut db = self.db.clone();
        Post::filter_by_id(post_id)
            .update()
            .title(title)
            .exec(&mut db)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(1)
    }

    async fn delete_post(&self, post_id: i32) -> BenchResult<u64> {
        let mut db = self.db.clone();
        Post::filter_by_id(post_id)
            .delete()
            .exec(&mut db)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(1)
    }

    async fn count_posts(&self, user_id: i32) -> BenchResult<i64> {
        let mut db = self.db.clone();
        let count = Post::filter_by_user_id(user_id)
            .count()
            .exec(&mut db)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(count as i64)
    }

    async fn sum_views(&self, _user_id: i32) -> BenchResult<i64> {
        Err(BenchError::Unsupported("toasty has no SUM aggregate"))
    }

    async fn tx_user_with_posts(&self, email: String, posts: usize) -> BenchResult<i32> {
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(anyhow::Error::from)?;

        let user = toasty::create!(User {
            email: email,
            username: "tx-user",
        })
        .exec(&mut tx)
        .await
        .map_err(anyhow::Error::from)?;

        for i in 0..posts {
            toasty::create!(Post {
                user_id: user.id,
                title: format!("tx post {i}"),
                content: bench_core::scenario::BODY,
                views: i as i32,
            })
            .exec(&mut tx)
            .await
            .map_err(anyhow::Error::from)?;
        }

        tx.commit().await.map_err(anyhow::Error::from)?;
        Ok(user.id)
    }
}
