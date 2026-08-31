---
title: Introduction
description: A lightweight Rust ORM that generates a typed client from a .bo schema
order: 0
---

# ByteORM

ByteORM generates a **fully typed client crate** from a schema file. You describe your
models once, run one command, and get Rust code where every table, column and enum is a
real type the compiler knows about.

```bo
model User {
    id         BigInt      PrimaryKey
    email      String      Unique
    username   String      NotNull
    created_at TimestamptZ @default(now())
}
```

```rust
let user = client.user.create(|u| {
    u.set_email("alice@example.com")
        .set_username("alice")
}).await?;
```

Every table, column and enum reaches your code as a real type. Misspell a column and the
build fails.

## How it fits together

1. **Write a schema** in a `.bo` file. The syntax is close to Prisma's.
2. **Generate a client** with `byteorm generate`, or `byteorm push` to apply the schema
   to your database at the same time.
3. **Depend on the generated crate** and call it from your code.

The generated crate is self-contained: it vendors a copy of the macros matched to the
version of ByteORM that produced it, so there is nothing to keep in sync by hand.

:::tip
New here? [Installation](/getting-started/installation) then
[Quickstart](/getting-started/quickstart) gets you from nothing to a working query.
:::

## What it is not

ByteORM targets **PostgreSQL** and does not try to abstract over other databases. That is
deliberate: an abstraction wide enough to cover every engine ends up hiding what makes any
one of them worth using.

It is also not a migration framework with a version history. `byteorm push` compares your
schema to the live database and applies the difference, refusing anything destructive
unless you say otherwise.

## Where to go next

- [Schema reference](/schema): models, enums, types, relations
- [CLI reference](/cli): every command and flag
- [Using the client](/client): CRUD, queries, transactions
