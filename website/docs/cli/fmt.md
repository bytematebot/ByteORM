---
title: fmt
description: Format schema files
order: 5
---

# fmt

```sh
byteorm fmt
byteorm fmt --check
```

Rewrites `.bo` files into a canonical layout: aligned columns, consistent spacing.

`--check` writes nothing and fails if any file is not already formatted, which is the form
to use in CI.
