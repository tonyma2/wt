---
paths: ["src/cli.rs"]
---

## Doc comment coupling

`///` on positional args in this file serves two purposes:
1. clap derives `--help` text from them
2. Zsh completion uses string replacement to inject dynamic completers — replacement targets are these doc comments

Each positional arg used as a replacement target must have a **unique** doc comment so targets don't collide. See the `*_TARGET` constants in `commands/init.rs` for the full set. After editing arg help text, run the zsh completion tests to verify completions still work.
