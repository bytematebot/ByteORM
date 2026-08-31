---
title: Upgrading
description: Keeping the CLI and the generated client in step
order: 2
---

# Upgrading

```sh
byteorm self-update
```

Or re-run the install command, which does the same thing.

## What happens to the generated client

The generated `Cargo.toml` records the version that produced it. After an upgrade, the next
[`generate`](/cli/generate) or [`push`](/cli/push) sees the mismatch and **rewrites the
whole client**, even when the schema has not changed.

That is deliberate. The client contains vendored macros, and those must match the runtime
they were generated against. Refreshing everything at once removes the possibility of a
half-upgraded client that compiles but misbehaves.

When the versions do match, only files whose content actually changed are written, so Cargo
does not rebuild for nothing.

```sh
byteorm generate --force
```

forces the full rewrite at any time.

## Checklist

1. `byteorm self-update`
2. `byteorm generate` (or `push`)
3. `cargo build`, where a broken call site shows up as a compile error
