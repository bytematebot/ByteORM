//! Toasty models. The struct *is* the schema: derive attributes drive both
//! the table mapping and the generated query methods.

#[derive(Debug, toasty::Model)]
#[table = "users"]
pub struct User {
    #[key]
    #[auto]
    pub id: i32,

    #[unique]
    pub email: String,

    pub username: String,

    #[auto]
    pub created_at: toasty::stmt::Timestamp,

    #[has_many]
    pub posts: toasty::Deferred<Vec<Post>>,
}

#[derive(Debug, toasty::Model)]
#[table = "posts"]
pub struct Post {
    #[key]
    #[auto]
    pub id: i32,

    #[index]
    pub user_id: i32,

    pub title: String,

    pub content: String,

    pub views: i32,

    #[auto]
    pub created_at: toasty::stmt::Timestamp,

    #[belongs_to(key = user_id, references = id)]
    pub user: toasty::Deferred<User>,
}
