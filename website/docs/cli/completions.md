---
title: completions
description: Shell autocomplete
order: 7
---

# completions

```sh
byteorm completions            # detect the shell, print the install command
byteorm completions install    # install for the detected shell
byteorm completions bash
byteorm completions zsh
byteorm completions fish
byteorm completions powershell
byteorm completions elvish
```

`install` writes the script to the standard location for the detected shell.

For a single session without installing anything:

```sh
source <(byteorm completions bash)
```
