---
title: Queries
description: Filters, ordering, paging and aggregates
order: 2
---

# Queries

## Filters

Every field produces a family of `where_` methods:

| Method | Matches |
| ------ | ------- |
| `where_field(value)` | Equal to `value` |
| `where_field_eq(value)` | The same, spelled out |
| `where_field_gt` / `where_field_gte` | Greater than, greater or equal |
| `where_field_lt` / `where_field_lte` | Less than, less or equal |
| `where_field_in(values)` | Any of `values` |
| `where_field_is_null` | `NULL` |
| `where_field_is_not_null` | Not `NULL` |

```rust
let recent = client.post.find_many(|p| {
    p.where_status(PostStatus::Published)
        .where_created_at_gte(cutoff)
}).await?;
```

Conditions combine with `AND`.

For something the builder cannot express, drop to SQL for that one clause:

```rust
p.where_raw("lower(title) LIKE $1", vec!["%rust%".into()])
```

## Ordering

```rust
p.order_by_created_at_desc()
p.order_by_title_asc()
```

Call more than once to order by several columns, in the order given.

## Paging

```rust
p.limit(20).offset(40)
```

`take` is accepted as a synonym for `limit`.

## Relations

```rust
p.include_user()
```

Fetches the referenced row alongside the parent. See [Relations](/schema/relations).

## Aggregates

```rust
let total   = client.post.count(|p| p.where_user_id(id)).await?;
let sum     = client.post.sum(|p| p.where_user_id(id)).await?;
let average = client.post.avg(|p| p.where_user_id(id)).await?;
let oldest  = client.post.min(|p| p.where_user_id(id)).await?;
let newest  = client.post.max(|p| p.where_user_id(id)).await?;
```
