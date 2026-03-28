# CLAUDE.md

> **Start here.** This file is the map — operational rules and pointers to deeper docs.

## Docs

- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — module graph, key types, data flow. Read before exploring unfamiliar modules
- **[docs/BACKLOG.md](docs/BACKLOG.md)** — prioritized improvement backlog
- **[docs/decisions.md](docs/decisions.md)** — You MUST read this file before refactoring or changing existing patterns

## Build & Test

```sh
cargo build                                # compile
cargo test                                 # all tests (unit + integration)
cargo test --test new                      # one integration test file
cargo test --test new -- create            # filter by test name
cargo fmt                                  # formatting
cargo clippy --all-targets -- -D warnings  # lints
```

`cargo test`, `cargo fmt`, and `cargo clippy` must all pass before work is complete.

## Structure

- One file per subcommand in `commands/`, each exports `pub fn run(...) -> Result<(), String>`
- All git calls go through the `Git` struct (`git.rs`)

## Style

- Follow Rust 2024 conventions unless there is a strong reason not to
- Self-documenting code — comments welcome when they explain a non-obvious *why*, but never restate what the code does
- Combinators (`map_err`, `and_then`, `is_ok_and`, `is_some_and`) over match when clearer
- Imports grouped: std → external crates → `crate::`, separated by blank lines
- `let`-chains for multi-condition guards
- Bool struct fields have no `is_` prefix — `bare`, `locked`, `prunable`
- Canonicalize paths before using as map keys or persisting to config (`/tmp` → `/private/tmp` on macOS)

## Tests

Every new feature, bug fix, or behavioral change MUST include tests.
