---
title: doctor
description: Show what the CLI actually resolved
order: 4
---

# doctor

```sh
byteorm doctor
```

Prints the config it loaded, the schema files it found, and the output paths it will use.

Run it first whenever something lands in an unexpected place: a schema that is not picked
up, a client written to the wrong directory, or a config that is not the one you edited.
Most of those are path questions, and this answers them directly instead of by inference.
