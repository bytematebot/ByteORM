---
title: CLI
order: 3
pager: false
---

# CLI

```
byteorm <command> [options]
```

| Command | What it does |
| ------- | ------------ |
| [`init`](/cli/init) | Write `byteorm.toml`, a starter schema, and an ignore entry |
| [`generate`](/cli/generate) | Build the client crate from the schema, no database needed |
| [`push`](/cli/push) | Apply the schema to the database and regenerate the client |
| [`doctor`](/cli/doctor) | Print resolved config, schema files and output paths |
| [`fmt`](/cli/fmt) | Format schema files |
| [`repair`](/cli/repair) | Add missing constraints and indexes from the schema |
| `reset` | Drop every table and reset state |
| `self-update` | Update the CLI from GitHub |
| [`completions`](/cli/completions) | Generate or install shell completions |

## Global options

These work on any command and override `byteorm.toml`:

| Option | Meaning |
| ------ | ------- |
| `--config <PATH>` | Path to `byteorm.toml` |
| `--schema <PATH>` | Use a single schema file |
| `--schema-dir <DIR>` | Use every `.bo` file in a directory |
| `--output <DIR>` | Write the client crate here |

:::danger
`byteorm reset` drops all database tables. There is no undo.
:::
