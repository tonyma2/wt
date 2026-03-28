---
paths: ["tests/**"]
---

## What to test

Observable behavior — exit codes, stdout/stderr content, filesystem side effects. Cover the happy path and likely failure modes (missing args, conflicting state, dirty worktree).

- **Integration tests** (`tests/`): one file per subcommand, run the compiled binary against real temp git repos
- **Unit tests**: inline under `#[cfg(test)] mod tests` for pure parsing/logic
- **No mocking**: tests use real git repos in temp directories (see docs/decisions.md)

## Test helpers

`tests/common/mod.rs` has setup, runner, and assertion helpers. Read it before writing tests — use existing helpers rather than reinventing them.
