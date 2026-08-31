---
title: Enums
description: Database enum types and their Rust counterparts
order: 2
---

# Enums

```bo
enum PostStatus {
    Draft
    Published
    Archived
}
```

An enum becomes a PostgreSQL enum type and a Rust enum in the generated crate:

```rust
use byteorm_client::PostStatus;

client.post.update(|p| {
    p.where_id(id).set_status(PostStatus::Published)
}).await?;
```

Use one as a field type like any other:

```bo
model Post {
    status PostStatus @default(Draft)
}
```

:::warning
Removing a variant, or removing the enum itself, is a destructive change.
`byteorm push` will refuse it until you pass `--accept-data-loss`.
:::
