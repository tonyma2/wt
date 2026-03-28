---
paths: ["src/cli.rs"]
---

## Doc comment coupling

`///` on args in this file serves two purposes:
1. clap derives `--help` text from them
2. Zsh completion uses string replacement to inject dynamic completers — replacement targets are these doc comments

Each subcommand's `name`/`names`/`base` arg must have a **unique** doc comment so replacement targets don't collide. After editing arg help text, run `cargo test -- zsh_completion_is_dynamic` to verify completions still work.
