---
title: Installation
description: Install the ByteORM CLI from the repository
order: 1
---

# Installation

ByteORM is installed **from the repository**, not from crates.io.

```sh
curl -fsSL https://raw.githubusercontent.com/bytematebot/byteorm/main/install.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/bytematebot/byteorm/main/install.ps1 | iex
```

Both scripts run `cargo install --git`, so Rust and Cargo have to be present first.

To skip the scripts entirely:

```sh
cargo install --git https://github.com/bytematebot/byteorm \
  --package byteorm --bin byteorm --force
```

:::warning
The `byteorm` package on crates.io holds old `0.1.x` builds that cannot generate a working
client, and is not maintained. Install from git.
:::

## Updating

The install command is also the update command. Or, from an installed CLI:

```sh
byteorm self-update
```

## Shell completions

The install scripts detect your shell and offer to set up completions. To control that
without a prompt, set `BYTEORM_INSTALL_COMPLETIONS=1` to install or `0` to skip.

Afterwards you can manage completions yourself:

```sh
byteorm completions          # detect the shell and print the install command
byteorm completions install  # write the script to the standard location
```

See [Completions](/cli/completions) for the supported shells.

## Checking the install

```sh
byteorm doctor
```

This prints the resolved config, the schema files it found, and where the client will be
written. It is the first thing to run when something is not where you expect.
