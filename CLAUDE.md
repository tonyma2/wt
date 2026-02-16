# CLAUDE.md

> **Start here.** This file is the map — operational rules and pointers to deeper docs.

## Docs

- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — module graph, key types, data flow, commands, test harness
- **[docs/decisions.md](docs/decisions.md)** — intentional choices that look improvable but aren't (read before refactoring)
- **[docs/git-workflow.md](docs/git-workflow.md)** — commits, PRs, CI, merging

## Build & Test

```sh
cargo build                       # compile
cargo test                        # all tests (unit + integration)
cargo test --test new             # one integration test file
cargo test --test new -- create   # filter by test name
cargo fmt --check                 # formatting
cargo clippy -- -D warnings       # lints
```

## Structure

- One file per subcommand in `commands/`, each exports `pub fn run(...) -> Result<(), String>`
- All git calls go through the `Git` struct (`git.rs`)

## Style

- Follow Rust 2024 conventions unless there is a strong reason not to
- Self-documenting code — no comments unless explaining a non-obvious *why*
- No doc comments except in `cli.rs` (clap derives help text from `///`)
- `Result<(), String>` everywhere — no error crates. Errors are human-readable strings
- Combinators (`map_err`, `and_then`, `is_ok_and`) over match when clearer
- `let`-chains for multi-condition guards
- Bool struct fields have no `is_` prefix — `bare`, `locked`, `prunable`

## CLI Norms

- **stdout**: data only — must be parseable by scripts
- **stderr**: `wt: lowercase message` (no period)
- **Errors**: lowercase, no period, "cannot" not "failed to", actionable ("use --force")
- **Exit codes**: 0 success, 1 error, 2 usage (clap)

## Tests

Every new feature, bug fix, or behavioral change MUST include tests. Work is not complete until `cargo test` passes.

- **What to test**: observable behavior — exit codes, stdout/stderr content, filesystem side effects. Cover the happy path and likely failure modes (missing args, conflicting state, dirty worktree)
- **Integration tests** (`tests/`): one file per subcommand, run the compiled binary against real temp git repos. See [ARCHITECTURE.md § Test Harness](docs/ARCHITECTURE.md#test-harness) for setup helpers and assertions
- **Unit tests**: inline under `#[cfg(test)] mod tests` for pure parsing/logic
- **No mocking**: tests use real git repos in temp directories. Do not introduce mock layers (see [decisions.md](docs/decisions.md))

## Git Workflow

Conventional commits, squash merge, branch naming: `<type>/<description>`. Full procedure in **[docs/git-workflow.md](docs/git-workflow.md)**.
