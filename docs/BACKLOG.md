# Backlog

**P1** now · **P2** later · **P3** maybe

**S** quick · **M** hours · **L** days

Delete when shipped.

## Features

- **`wt clone`** (P1, L) — `wt clone <url>` → bare clone + first worktree + correct fetch refspec. One opinionated flow, don't replicate `git clone`'s flag surface. Kills onboarding friction.
- **Global status view** (P1, M) — cross-repo worktree summary via `~/.wt/worktrees/` discovery (same mechanism as global `prune`). Could be `wt list --all` or a separate subcommand.

## Code quality

- **Re-validate paths in `auto_link()`** (P2, S) — `link.rs:auto_link()` creates symlinks from paths loaded from `~/.wt/config` without calling `validate_path()`. CLI-provided paths are validated before entering config, but a hand-edited config with `../` paths would bypass validation. Defense-in-depth: call `validate_path()` in the `auto_link()` loop body.
- **`remove_dest` should refuse to delete directories** (P2, S) — `link.rs:remove_dest()` calls `remove_dir_all` when the destination is a real directory. With `--force`, a file-to-directory change across branches could cause silent recursive deletion of an entire directory tree. `remove_dest` should error on real directories instead of deleting them — it only needs to handle files and symlinks.

## Documentation

- **Shell completion install instructions** (P1, S) — `wt init <shell>` works but docs don't explain where to add the `eval` line per shell, or troubleshoot if completions don't load.
