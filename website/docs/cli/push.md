---
title: push
description: Apply the schema to the database and regenerate
order: 3
---

# push

```sh
byteorm push
byteorm push --dry-run
byteorm push --accept-data-loss
byteorm push --force
```

Compares the schema to the live database, applies the difference, and regenerates the
client in the same step.

| Flag | Effect |
| ---- | ------ |
| `--dry-run` | Print the migration SQL and generate the client, without executing anything |
| `--accept-data-loss` | Allow destructive changes |
| `--force` | Rewrite every generated file, changed or not |

## Destructive changes

Dropping a table, a column or an enum type destroys data, so `push` stops rather than
guessing that you meant it:

```sh
byteorm push --accept-data-loss
```

:::warning
Read the `--dry-run` output before accepting data loss. It is the same migration, printed
instead of executed.
:::

## Constraints drifting

If the tables are right but a unique constraint or index is missing, as happens with a
database restored from an older dump, [`repair`](/cli/repair) adds what the schema calls
for without a full migration.
