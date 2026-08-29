//! Row structs. SQLx maps them by column name through `FromRow`; there is no
//! schema DSL to keep in sync, which is the whole point of a SQL-first
//! library.

use bench_core::{PostRow, UserRow};
use chrono::{DateTime, Utc};

#[derive(sqlx::FromRow)]
pub struct User {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct Post {
    pub id: i32,
    pub user_id: i32,
    pub title: String,
    pub content: String,
    pub views: i32,
    pub created_at: DateTime<Utc>,
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
