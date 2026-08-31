---
title: init
description: Create the config and a starter schema
order: 1
---

# init

```sh
byteorm init
```

Creates `byteorm.toml`, a starter `schema.bo`, and an ignore entry for the generated
client. If the project already uses `byteorm/*.bo` or `generated/`, that layout is kept
rather than replaced.

## The config it writes

```toml
[schema]
path = "schema.bo"

[client]
output = ".byteorm/client"
crate_name = "byteorm-client"
dependency_source = "vendored"
```

| Key | Meaning |
| --- | ------- |
| `schema.path` | A single schema file |
| `schema.directory` | A directory of `.bo` files, instead of `path` |
| `client.output` | Where the generated crate is written |
| `client.crate_name` | The name your `Cargo.toml` will depend on |
| `client.dependency_source` | `vendored` ships the macros inside the client |

For a project with several schema files, see [Multiple schemas](/guides/multi-schema).
