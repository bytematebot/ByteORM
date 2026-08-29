//! ByteORM adapter, driving the client crate generated from `schema.bo`.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bench_core::{BenchError, BenchResult, NewPost, OrmAdapter, PostRow, UserRow};
use byteorm_client::{Client, Posts, Users, tokio_postgres};

pub struct ByteOrmAdapter {
    client: Client,
}

pub async fn connect(url: String, _pool_size: u32) -> Result<Arc<dyn OrmAdapter>> {
    // ByteORM sizes its bb8 pool internally; the runner's `--pool-size` is
    // reported in the meta section so the difference stays visible.
    let client = Client::new(&url).await?;
    Ok(Arc::new(ByteOrmAdapter { client }))
}

fn user_row(u: Users) -> UserRow {
    UserRow {
        id: u.id,
        email: u.email,
        username: u.username,
        created_at: u.created_at,
    }
}

fn post_row(p: Posts) -> PostRow {
    PostRow {
        id: p.id,
        user_id: p.user_id,
        title: p.title,
        content: p.content,
        views: p.views,
        created_at: p.created_at,
    }
}

/// The generated client returns boxed errors; the harness wants one type.
fn boxed(e: Box<dyn std::error::Error + Send + Sync>) -> BenchError {
    BenchError::Other(anyhow::anyhow!(e.to_string()))
}

#[async_trait]
impl OrmAdapter for ByteOrmAdapter {
    fn name(&self) -> &'static str {
        "byteorm"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn insert_user(&self, email: String, username: String) -> BenchResult<i32> {
        let user = self
            .client
            .users
            .create(|u| u.set_email(email).set_username(username))
            .await
            .map_err(boxed)?;
        Ok(user.id)
    }

    async fn insert_posts_many(&self, rows: Vec<NewPost>) -> BenchResult<u64> {
        let records = rows
            .into_iter()
            .map(|r| {
                let mut record: std::collections::HashMap<
                    &'static str,
                    Box<dyn tokio_postgres::types::ToSql + Sync + Send>,
                > = std::collections::HashMap::new();
                record.insert("user_id", Box::new(r.user_id));
                record.insert("title", Box::new(r.title));
                record.insert("content", Box::new(r.content));
                record.insert("views", Box::new(r.views));
                record
            })
            .collect();
        self.client.posts.create_many(records).await.map_err(boxed)
    }

    async fn find_user_by_id(&self, id: i32) -> BenchResult<Option<UserRow>> {
        let user = self.client.users.find_unique(id).await.map_err(boxed)?;
        Ok(user.map(user_row))
    }

    async fn find_user_by_email(&self, email: &str) -> BenchResult<Option<UserRow>> {
        let user = self
            .client
            .users
            .find_first(|u| u.where_email(email.to_string()))
            .await
            .map_err(boxed)?;
        Ok(user.map(user_row))
    }

    async fn recent_posts(&self, user_id: i32, limit: i64) -> BenchResult<Vec<PostRow>> {
        let posts = self
            .client
            .posts
            .find_many(|p| {
                p.where_user_id(user_id)
                    .order_by_created_at_desc()
                    .limit(limit as usize)
            })
            .await
            .map_err(boxed)?;
        Ok(posts.into_iter().map(post_row).collect())
    }

    async fn update_post_title(&self, post_id: i32, title: String) -> BenchResult<u64> {
        self.client
            .posts
            .update(|p| p.where_id(post_id).set_title(title))
            .await
            .map_err(boxed)?;
        Ok(1)
    }

    async fn delete_post(&self, post_id: i32) -> BenchResult<u64> {
        self.client
            .posts
            .delete(|p| p.where_id(post_id))
            .await
            .map_err(boxed)
    }

    async fn count_posts(&self, user_id: i32) -> BenchResult<i64> {
        self.client
            .posts
            .count(|p| p.where_user_id(user_id))
            .await
            .map_err(boxed)
    }

    async fn sum_views(&self, user_id: i32) -> BenchResult<i64> {
        self.client
            .posts
            .sum_views(|p| p.where_user_id(user_id))
            .await
            .map_err(boxed)
    }

    async fn tx_user_with_posts(&self, email: String, posts: usize) -> BenchResult<i32> {
        let tx = self.client.begin().await?;
        let user = tx
            .users
            .create(|u| u.set_email(email).set_username("tx-user".to_string()))
            .await
            .map_err(boxed)?;
        for i in 0..posts {
            tx.posts
                .create(|p| {
                    p.set_user_id(user.id)
                        .set_title(format!("tx post {i}"))
                        .set_content(bench_core::scenario::BODY.to_string())
                        .set_views(i as i32)
                })
                .await
                .map_err(boxed)?;
        }
        tx.commit().await?;
        Ok(user.id)
    }
}
