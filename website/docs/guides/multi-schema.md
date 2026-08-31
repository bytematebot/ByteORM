---
title: Multiple schemas
description: Splitting a large schema across files
order: 1
---

# Multiple schemas

Point the config at a directory instead of a file, and every `.bo` inside it is loaded:

```toml
[schema]
directory = "byteorm"

[client]
output = "generated"
crate_name = "byteorm-client"
dependency_source = "vendored"
```

```
byteorm/
├── users.bo
├── posts.bo
└── billing.bo
```

The files are one schema, not several. A model in `posts.bo` can reference a model in
`users.bo` with no import or declaration order to manage.

One client crate is generated for the whole set.

## Overriding per command

```sh
byteorm generate --schema-dir byteorm --output generated
byteorm generate --schema schema.bo
```

Useful for a one-off run without editing the config. `byteorm doctor` prints which of them
actually took effect.
