//! SeaORM adapter.

pub mod entity;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use bench_core::{BenchResult, NewPost, OrmAdapter, PostRow, UserRow};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};

use entity::{posts, users};

pub struct SeaOrmAdapter {
    db: DatabaseConnection,
}

pub async fn connect(url: String, pool_size: u32) -> Result<Arc<dyn OrmAdapter>> {
    let mut opts = ConnectOptions::new(url);
    opts.max_connections(pool_size)
        .min_connections(1)
        .sqlx_logging(false)
        .connect_timeout(Duration::from_secs(10));
    let db = Database::connect(opts).await?;
    Ok(Arc::new(SeaOrmAdapter { db }))
}

impl From<users::Model> for UserRow {
    fn from(u: users::Model) -> Self {
        UserRow {
            id: u.id,
            email: u.email,
            username: u.username,
            created_at: u.created_at,
        }
    }
}

impl From<posts::Model> for PostRow {
    fn from(p: posts::Model) -> Self {
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
impl OrmAdapter for SeaOrmAdapter {
    fn name(&self) -> &'static str {
        "sea-orm"
    }

    fn version(&self) -> &'static str {
        "2.0"
    }

    async fn insert_user(&self, email: String, username: String) -> BenchResult<i32> {
        let model = users::ActiveModel {
            email: Set(email),
            username: Set(username),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(model.id)
    }

    async fn insert_posts_many(&self, rows: Vec<NewPost>) -> BenchResult<u64> {
        let count = rows.len() as u64;
        let models = rows.into_iter().map(|r| posts::ActiveModel {
            user_id: Set(r.user_id),
            title: Set(r.title),
            content: Set(r.content),
            views: Set(r.views),
            ..Default::default()
        });
        // `insert_many` reports only the last insert id, so the row count comes
        // from the input; the statement itself is one round trip either way.
        posts::Entity::insert_many(models).exec(&self.db).await?;
        Ok(count)
    }

    async fn find_user_by_id(&self, id: i32) -> BenchResult<Option<UserRow>> {
        let user = users::Entity::find_by_id(id).one(&self.db).await?;
        Ok(user.map(Into::into))
    }

    async fn find_user_by_email(&self, email: &str) -> BenchResult<Option<UserRow>> {
        let user = users::Entity::find()
            .filter(users::COLUMN.email.eq(email))
            .one(&self.db)
            .await?;
        Ok(user.map(Into::into))
    }

    async fn recent_posts(&self, user_id: i32, limit: i64) -> BenchResult<Vec<PostRow>> {
        let rows = posts::Entity::find()
            .filter(posts::COLUMN.user_id.eq(user_id))
            .order_by_desc(posts::COLUMN.created_at)
            .limit(limit as u64)
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn update_post_title(&self, post_id: i32, title: String) -> BenchResult<u64> {
        let result = posts::Entity::update_many()
            .col_expr(posts::COLUMN.title, Expr::value(title))
            .filter(posts::COLUMN.id.eq(post_id))
            .exec(&self.db)
            .await?;
        Ok(result.rows_affected)
    }

    async fn delete_post(&self, post_id: i32) -> BenchResult<u64> {
        let result = posts::Entity::delete_by_id(post_id).exec(&self.db).await?;
        Ok(result.rows_affected)
    }

    async fn count_posts(&self, user_id: i32) -> BenchResult<i64> {
        let count = posts::Entity::find()
            .filter(posts::COLUMN.user_id.eq(user_id))
            .count(&self.db)
            .await?;
        Ok(count as i64)
    }

    async fn sum_views(&self, user_id: i32) -> BenchResult<i64> {
        let sum: Option<i64> = posts::Entity::find()
            .filter(posts::COLUMN.user_id.eq(user_id))
            .select_only()
            .column_as(posts::COLUMN.views.sum(), "total")
            .into_tuple::<Option<i64>>()
            .one(&self.db)
            .await?
            .flatten();
        Ok(sum.unwrap_or(0))
    }

    async fn tx_user_with_posts(&self, email: String, posts_count: usize) -> BenchResult<i32> {
        let tx = self.db.begin().await?;
        let user = users::ActiveModel {
            email: Set(email),
            username: Set("tx-user".to_string()),
            ..Default::default()
        }
        .insert(&tx)
        .await?;
        for i in 0..posts_count {
            posts::ActiveModel {
                user_id: Set(user.id),
                title: Set(format!("tx post {i}")),
                content: Set(bench_core::scenario::BODY.to_string()),
                views: Set(i as i32),
                ..Default::default()
            }
            .insert(&tx)
            .await?;
        }
        tx.commit().await?;
        Ok(user.id)
    }
}
