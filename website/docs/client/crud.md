---
title: CRUD
description: Create, read, update and delete
order: 1
---

# CRUD

Every repository method takes a closure that builds the operation. The closure receives a
builder whose methods come from your schema.

## Create

```rust
let user = client.user.create(|u| {
    u.set_email("alice@example.com")
        .set_username("alice")
}).await?;
```

Missing a required column raises a runtime error that names the column.

Insert many rows in one statement:

```rust
client.user.create_many(|u| { /* ... */ }).await?;
```

## Read

| Method | Returns |
| ------ | ------- |
| `find_many` | Every matching row |
| `find_first` | The first match, or `None` |
| `find_unique` | A row by a unique column |
| `find_by_id` | A row by primary key |
| `find_by_composite_pk` | A row by a multi-column primary key |
| `find_or_create` | The match, inserting it when absent |

```rust
let post = client.post.find_by_id(id).await?;

let posts = client.post.find_many(|p| {
    p.where_user_id(user.id).order_by_created_at_desc()
}).await?;
```

## Update

```rust
client.post.update(|p| {
    p.where_id(post.id).set_status(PostStatus::Published)
}).await?;
```

An update without a filter would rewrite the whole table, so the builder requires you to
opt into that explicitly rather than doing it by omission.

Numeric columns can be changed in place, which avoids reading the value first:

```rust
client.post.update(|p| p.where_id(id).inc_views(1)).await?;
```

`inc_`, `dec_`, `mul_` and `div_` are generated for every numeric field.

## Upsert

```rust
client.user.upsert(|u| { /* ... */ }).await?;
client.user.upsert_many(|u| { /* ... */ }).await?;
```

## Delete

```rust
client.post.delete(|p| p.where_id(post.id)).await?;
```
