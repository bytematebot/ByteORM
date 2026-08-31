---
title: Editor support
description: Syntax highlighting for .bo files
order: 3
---

# Editor support

## IntelliJ

The plugin lives in `integrations/intellij/byteorm-intellij/` in the repository. It
provides syntax highlighting for `.bo` files, colouring keywords, types, modifiers,
attributes, declaration names and literals separately.

It is kept as a normal subproject rather than a git submodule, so the editor code stays
next to ByteORM while remaining easy to extract later into its own repository for
IntelliJ, VS Code, Zed or anything else.

:::info
The code blocks in these docs use the same palette as the plugin, so a schema reads the
same here as it does in the editor.
:::

## Other editors

Nothing to install yet. The plugin's lexer is the reference for what a highlighter needs
to recognise: `model` and `enum` as keywords, the built-in [types](/schema/types), the
field modifiers, `@`-prefixed attributes, `//` comments and `?` as the optional marker.
