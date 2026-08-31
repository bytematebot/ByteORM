---
title: generate
description: Build the client crate without a database
order: 2
---

# generate

```sh
byteorm generate
byteorm generate --force
```

Reads the schema and writes the client crate. **No database connection is used**, which
makes it the command for CI, for a fresh checkout, and for any machine that cannot reach
the database.

## What gets written

Only files whose content actually changed. Everything else keeps its modification time, so
Cargo does not rebuild the client for nothing.

`--force` rewrites every file regardless.

## Version locking

The generated `Cargo.toml` records which ByteORM produced it. When you upgrade the CLI,
the next `generate` or [`push`](/cli/push) notices the mismatch and rewrites the whole
client even if the schema is untouched. The runtime, the vendored macros and the CLI
always come from one version.

See [Upgrading](/guides/upgrading).
