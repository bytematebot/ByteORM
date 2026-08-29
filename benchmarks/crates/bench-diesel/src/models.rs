//! Queryable/Insertable structs, the second half of Diesel's schema story.

use chrono::{DateTime, Utc};
use diesel::prelude::*;

use crate::schema::{posts, users};

#[derive(Queryable, Selectable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub email: &'a str,
    pub username: &'a str,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = posts)]
pub struct Post {
    pub id: i32,
    pub user_id: i32,
    pub title: String,
    pub content: String,
    pub views: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable)]
#[diesel(table_name = posts)]
pub struct NewPostRow {
    pub user_id: i32,
    pub title: String,
    pub content: String,
    pub views: i32,
}
