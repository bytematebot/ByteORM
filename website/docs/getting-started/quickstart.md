---
title: Quickstart
description: From an empty project to a typed query
order: 2
---

# Quickstart

## 1. Initialize

```sh
byteorm init
```

This writes `byteorm.toml`, a starter `schema.bo`, and an ignore entry for the generated
client. Projects that already use `byteorm/*.bo` or `generated/` keep the layout they
have.

```toml
[schema]
path = "schema.bo"

[client]
output = ".byteorm/client"
crate_name = "byteorm-client"
dependency_source = "vendored"
```

## 2. Describe your models

```bo
enum PostStatus {
    Draft
    Published
    Archived
}

model User {
    id         BigInt      PrimaryKey
    email      String      Unique
    username   String      NotNull
    created_at TimestamptZ @default(now())
}

model Post {
    id         BigInt      PrimaryKey
    user_id    BigInt      NotNull ForeignKey(User.id, onDelete: cascade)
    title      String      NotNull
    content    String      NotNull
    status     PostStatus  @default(Draft)
    created_at TimestamptZ @default(now()) Index
    updated_at TimestamptZ @default(now())
}
```

The full syntax is in the [schema reference](/schema).

## 3. Generate or push

Generate the client without touching a database:

```sh
byteorm generate
```

Or apply the schema to the database and regenerate in one step:

```sh
byteorm push
```

`push` refuses to drop tables, columns or enum types unless you accept it explicitly.
See [push](/cli/push).

## 4. Depend on the client

Add the path that `init` or `push` printed:

```toml
[dependencies]
byteorm-client = { path = ".byteorm/client" }
```

## 5. Query

```rust
use byteorm_client::{Client, PostStatus};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new("postgres://user:pass@localhost:5432/mydb").await?;

    let user = client.user.create(|u| {
        u.set_email("alice@example.com")
            .set_username("alice")
    }).await?;

    let post = client.post.create(|p| {
        p.set_user_id(user.id)
            .set_title("Hello ByteORM")
            .set_content("My first post")
    }).await?;

    let posts = client.post.find_many(|p| {
        p.where_user_id(user.id)
            .order_by_created_at_desc()
    }).await?;

    client.post.update(|p| {
        p.where_id(post.id)
            .set_status(PostStatus::Published)
    }).await?;

    println!("{} has {} post(s)", user.username, posts.len());
    Ok(())
}
```

Every method above is generated from your schema. `set_title` exists because `Post` has a
`title`; rename the column and the call stops compiling.

:::info
A complete working project lives in `examples/blog/` in the repository.
:::
