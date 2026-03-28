---
paths: ["tests/**"]
---

## What to test

Observable behavior — exit codes, stdout/stderr content, filesystem side effects. Cover the happy path and likely failure modes (missing args, conflicting state, dirty worktree).

- **Integration tests** (`tests/`): one file per subcommand, run the compiled binary against real temp git repos
- **Unit tests**: inline under `#[cfg(test)] mod tests` for pure parsing/logic
- **No mocking**: tests use real git repos in temp directories (see docs/decisions.md)

## Test helpers (tests/common/mod.rs)

- `setup()` → `(TempDir, repo_path)` — temp home + initialized repo
- `setup_with_origin()` → `(TempDir, repo_path, origin_path)` — adds bare remote
- `wt(home)` / `run_wt(home, |cmd| {...})` — run binary with isolated HOME
- `wt_new(home, repo, branch)` / `wt_checkout(home, repo, name)` — create worktrees
- `git(dir)` → raw git Command, `assert_git_success(dir, args)` for assertions
- `assert_exit_code`, `assert_stdout_exact`, `assert_stderr_exact`, `assert_error(output, code, stderr)`
- `canonical(path)` — resolve symlinks (important on macOS `/tmp` → `/private/tmp`)
