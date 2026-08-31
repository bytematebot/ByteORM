---
title: repair
description: Restore constraints the schema expects
order: 6
---

# repair

```sh
byteorm repair
```

Adds unique constraints and indexes that the schema declares but the database is missing.

This is for drift rather than for migration: a database restored from a dump that predates
an index, or one changed by hand. For schema changes, use [`push`](/cli/push).
