# CLAUDE.md

## Build & Test

```sh
cargo build                       # compile
cargo test                        # all tests (unit + integration)
cargo test --test integration     # integration tests only
cargo test -p wt -- new::         # filter by test path
```

## Architecture

Manages git worktrees under `~/.worktrees/<repo-name>/`.

- One file per subcommand in `commands/`, each exports `pub fn run(...) -> Result<(), String>`
- All git calls go through the `Git` struct with `-C <repo>`. Exceptions: `check_ref_format` (free fn, no repo needed), `is_dirty` (runs in worktree path, not admin repo)
- `parse_porcelain()` strips `refs/heads/` from branch names during parsing
- Data flow: `Git::list_worktrees()` → `parse_porcelain()` → query `Vec<Worktree>` in memory

## Style

- Self-documenting code. No comments unless explaining a non-obvious *why*. No doc comments except in `cli.rs` (clap derives help text from `///`).
- `Result<(), String>` everywhere — no error crates. Errors are human-readable strings.
- Combinators (`map_err`, `and_then`, `is_ok_and`) over match when clearer.
- `let`-chains for multi-condition guards.
- Bool struct fields have no `is_` prefix — `bare`, `locked`, `prunable`.

## CLI Norms

- **stdout**: data only — must be parseable by scripts
- **stderr**: `wt: lowercase message` (no period)
- **Errors**: lowercase, no period, "cannot" not "failed to", actionable ("use --force")
- **Exit codes**: 0 success, 1 error, 2 usage (clap)

## Tests

IMPORTANT: Every new feature, bug fix, or behavioral change MUST include tests. This project is maintained by AI agents — tests are the primary regression guard.

**What to test:** Observable behavior — exit codes, stdout/stderr content, filesystem side effects. Cover the happy path and likely failure modes (missing args, conflicting state, dirty worktree).

**Integration tests** (`tests/integration.rs`) — run the compiled binary against real temp git repos:

- `setup()` → `(TempDir, PathBuf)` — initialized repo, no remote. Keep `TempDir` in scope or the directory gets deleted.
- `wt_new(home, repo, branch)` — creates a worktree via the binary with `HOME` overridden
- New subcommand → new nested `mod` block with tests

**Unit tests** inline under `#[cfg(test)] mod tests` for pure parsing/logic.

Run `cargo test` after every change. All tests must pass before work is complete.
