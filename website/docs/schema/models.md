---
title: Models
description: Fields, keys, indexes and nullability
order: 1
---

# Models

A model becomes a table, a Rust struct, and a repository on the client.

```bo
model User {
    id         BigInt      PrimaryKey
    email      String      Unique
    username   String      NotNull
    bio        String?
    created_at TimestamptZ @default(now())
}
```

A field is a name, a [type](/schema/types), and any number of modifiers and attributes.

## Nullability

A type is non-null by default. Append `?` to allow `NULL`:

```bo
bio String?
```

The generated struct field becomes `Option<String>`, so the compiler forces you to handle
the empty case.

`NotNull` and `Nullable` are also accepted as explicit modifiers where you prefer to state
it outright.

## Keys

```bo
id       BigInt PrimaryKey
email    String Unique
```

`PrimaryKey` marks the identity of the row and enables `find_by_id`. `Unique` adds a
uniqueness constraint and makes the column usable with `find_unique`.

A composite primary key is declared by marking more than one field, which enables
`find_by_composite_pk`.

## Indexes

```bo
created_at TimestamptZ @default(now()) Index
```

`Index` creates a database index on the column. Add it to the columns you filter and sort
by, not to every column.

## Defaults

```bo
created_at TimestamptZ @default(now())
status     PostStatus  @default(Draft)
```

`@default(now())` uses the database clock. An enum default is written as the bare variant
name.

## Generated API

Every field produces methods on the query and mutation builders:

| Field   | Methods |
| ------- | ------- |
| `email` | `set_email`, `where_email`, `where_email_eq`, `where_email_in`, `where_email_is_null`, … |
| `created_at` | `order_by_created_at_asc`, `order_by_created_at_desc` |

Numeric fields also get `inc_`, `dec_`, `mul_` and `div_` for in-place arithmetic updates.
See [Queries](/client/queries) for the full set.
