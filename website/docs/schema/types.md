---
title: Types
description: The built-in field types
order: 3
---

# Types

| Schema type   | Postgres            | Notes |
| ------------- | ------------------- | ----- |
| `String`      | `varchar` / `text`  | Ordinary text |
| `Text`        | `text`              | Long-form text |
| `Int`         | `integer`           | 32-bit |
| `BigInt`      | `bigint`            | 64-bit; the usual choice for ids |
| `Serial`      | `serial`            | Auto-incrementing integer |
| `Float`       | `double precision`  | 64-bit floating point |
| `Real`        | `real`              | 32-bit floating point |
| `Boolean`     | `boolean`           | |
| `Date`        | `date`              | Date with no time |
| `TimestamptZ` | `timestamptz`       | Instant with a time zone |
| `JsonB`       | `jsonb`             | Binary JSON, also spelled `Jsonb` |

Any [enum](/schema/enums) you declare is usable as a type too.

Append `?` to any of them to make the column nullable.

## JSONB

`JsonB` columns get accessors on the client for reading and writing inside the document,
rather than forcing you to fetch and re-serialize the whole value:

```rust
let theme: String = client.user.settings.get_as(user_id, "theme").await?;
let present = client.user.settings.has(user_id, "beta").await?;
client.user.settings.set(user_id, "theme", "dark").await?;
```

Companion methods cover bulk reads: `get_all`, `get_many`, `get_many_as`, `get_many_ids`
and `get_or` for a fallback when the key is absent.
