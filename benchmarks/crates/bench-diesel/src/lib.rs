//! Diesel adapter, async flavour (`diesel-async` + bb8), so it shares the
//! tokio runtime with every other adapter instead of paying for
//! `spawn_blocking`.

pub mod models;
pub mod schema;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bench_core::{BenchResult, NewPost, OrmAdapter, PostRow, UserRow};
use diesel::prelude::*;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};

use models::{NewPostRow, NewUser, Post, User};
use schema::{posts, users};

pub struct DieselAdapter {
    pool: Pool<AsyncPgConnection>,
}

pub async fn connect(url: String, pool_size: u32) -> Result<Arc<dyn OrmAdapter>> {
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(url);
    let pool = Pool::builder().max_size(pool_size).build(config).await?;
    Ok(Arc::new(DieselAdapter { pool }))
}

impl From<User> for UserRow {
    fn from(u: User) -> Self {
        UserRow {
            id: u.id,
            email: u.email,
            username: u.username,
            created_at: u.created_at,
        }
    }
}

impl From<Post> for PostRow {
    fn from(p: Post) -> Self {
        PostRow {
            id: p.id,
            user_id: p.user_id,
            title: p.title,
            content: p.content,
            views: p.views,
            created_at: p.created_at,
        }
    }
}

#[async_trait]
impl OrmAdapter for DieselAdapter {
    fn name(&self) -> &'static str {
        "diesel-async"
    }

    fn version(&self) -> &'static str {
        "diesel 2.3 / diesel-async 0.9"
    }

    async fn insert_user(&self, email: String, username: String) -> BenchResult<i32> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let id = diesel::insert_into(users::table)
            .values(NewUser {
                email: &email,
                username: &username,
            })
            .returning(users::id)
            .get_result::<i32>(&mut conn)
            .await?;
        Ok(id)
    }

    async fn insert_posts_many(&self, rows: Vec<NewPost>) -> BenchResult<u64> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let values: Vec<NewPostRow> = rows
            .into_iter()
            .map(|r| NewPostRow {
                user_id: r.user_id,
                title: r.title,
                content: r.content,
                views: r.views,
            })
            .collect();
        let affected = diesel::insert_into(posts::table)
            .values(&values)
            .execute(&mut conn)
            .await?;
        Ok(affected as u64)
    }

    async fn find_user_by_id(&self, id: i32) -> BenchResult<Option<UserRow>> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let user = users::table
            .find(id)
            .select(User::as_select())
            .first::<User>(&mut conn)
            .await
            .optional()?;
        Ok(user.map(Into::into))
    }

    async fn find_user_by_email(&self, email: &str) -> BenchResult<Option<UserRow>> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let user = users::table
            .filter(users::email.eq(email))
            .select(User::as_select())
            .first::<User>(&mut conn)
            .await
            .optional()?;
        Ok(user.map(Into::into))
    }

    async fn recent_posts(&self, user_id: i32, limit: i64) -> BenchResult<Vec<PostRow>> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let rows = posts::table
            .filter(posts::user_id.eq(user_id))
            .order(posts::created_at.desc())
            .limit(limit)
            .select(Post::as_select())
            .load::<Post>(&mut conn)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn update_post_title(&self, post_id: i32, title: String) -> BenchResult<u64> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let affected = diesel::update(posts::table.find(post_id))
            .set(posts::title.eq(title))
            .execute(&mut conn)
            .await?;
        Ok(affected as u64)
    }

    async fn delete_post(&self, post_id: i32) -> BenchResult<u64> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let affected = diesel::delete(posts::table.find(post_id))
            .execute(&mut conn)
            .await?;
        Ok(affected as u64)
    }

    async fn count_posts(&self, user_id: i32) -> BenchResult<i64> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let count = posts::table
            .filter(posts::user_id.eq(user_id))
            .count()
            .get_result::<i64>(&mut conn)
            .await?;
        Ok(count)
    }

    async fn sum_views(&self, user_id: i32) -> BenchResult<i64> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let sum = posts::table
            .filter(posts::user_id.eq(user_id))
            .select(diesel::dsl::sum(posts::views))
            .get_result::<Option<i64>>(&mut conn)
            .await?;
        Ok(sum.unwrap_or(0))
    }

    async fn tx_user_with_posts(&self, email: String, posts_count: usize) -> BenchResult<i32> {
        let mut conn = self.pool.get().await.map_err(anyhow::Error::from)?;
        let id = conn
            .transaction::<i32, diesel::result::Error, _>(async |conn| {
                let user_id = diesel::insert_into(users::table)
                    .values(NewUser {
                        email: &email,
                        username: "tx-user",
                    })
                    .returning(users::id)
                    .get_result::<i32>(conn)
                    .await?;

                let rows: Vec<NewPostRow> = (0..posts_count)
                    .map(|i| NewPostRow {
                        user_id,
                        title: format!("tx post {i}"),
                        content: bench_core::scenario::BODY.to_string(),
                        views: i as i32,
                    })
                    .collect();
                for row in rows {
                    diesel::insert_into(posts::table)
                        .values(row)
                        .execute(conn)
                        .await?;
                }
                Ok(user_id)
            })
            .await?;
        Ok(id)
    }
}
