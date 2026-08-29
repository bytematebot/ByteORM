//! SeaORM 2.0 dense entity format: relations live on the model itself.

pub mod users {
    use chrono::{DateTime, Utc};
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "users")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        #[sea_orm(unique)]
        pub email: String,
        pub username: String,
        pub created_at: DateTime<Utc>,
        #[sea_orm(has_many)]
        pub posts: HasMany<super::posts::Entity>,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod posts {
    use chrono::{DateTime, Utc};
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "posts")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub user_id: i32,
        pub title: String,
        pub content: String,
        pub views: i32,
        pub created_at: DateTime<Utc>,
        #[sea_orm(belongs_to, from = "user_id", to = "id")]
        pub author: BelongsTo<super::users::Entity>,
    }

    impl ActiveModelBehavior for ActiveModel {}
}
