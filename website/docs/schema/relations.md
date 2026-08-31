---
title: Relations
description: Foreign keys and referential actions
order: 4
---

# Relations

```bo
model Post {
    id      BigInt PrimaryKey
    user_id BigInt NotNull ForeignKey(User.id, onDelete: cascade)
}
```

`ForeignKey` takes the target as `Model.field`, and optionally what should happen when the
referenced row is deleted.

## Referential actions

| Action      | Effect on delete |
| ----------- | ---------------- |
| `cascade`   | Delete the referencing rows too |
| `restrict`  | Refuse the delete while references exist |
| `set null`  | Clear the referencing column (the column must be nullable) |
| `no action` | Leave it to the database's own check |

```bo
user_id BigInt? ForeignKey(User.id, onDelete: set null)
```

## Querying across a relation

Each foreign key produces an `include_` method on the query builder, used by the JSON
reads to fetch the related row alongside the parent:

```rust
let posts = client.post.find_many(|p| {
    p.where_user_id(user.id).include_user()
}).await?;
```

Without an `include_`, a query returns only the columns of its own model.
