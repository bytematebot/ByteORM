---
title: Schema
order: 2
pager: false
---

# Schema

A ByteORM schema is one or more `.bo` files describing enums and models. It is the single
source of truth: the client crate and the database migrations are both derived from it.

```bo
enum PostStatus {
    Draft
    Published
}

model Post {
    id     BigInt     PrimaryKey
    title  String     NotNull
    status PostStatus @default(Draft)
}
```

- [Models](/schema/models): fields, keys, indexes, nullability
- [Enums](/schema/enums)
- [Types](/schema/types): the built-in field types
- [Relations](/schema/relations): foreign keys and referential actions

## Formatting

```sh
byteorm fmt          # rewrite schema files
byteorm fmt --check  # fail instead of writing, for CI
```
