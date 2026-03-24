# Backlog

**P1** now · **P2** later · **P3** maybe

**S** quick · **M** hours · **L** days

Delete when shipped.

## Features

- **`wt clone`** (P1, L) — `wt clone <url>` → bare clone + first worktree + correct fetch refspec. One opinionated flow, don't replicate `git clone`'s flag surface. Kills onboarding friction.
- **Global status view** (P1, M) — cross-repo worktree summary via `~/.wt/worktrees/` discovery (same mechanism as global `prune`). Could be `wt list --all` or a separate subcommand.

## Bugs

- **`prune` falsely prunes unpushed branches as "merged"** (P1, S) — `prune_merged` uses `is_ancestor(branch, base)` to detect merged branches, but a branch that was never pushed (`branch.<name>.remote` absent) can't have been merged via PR — squash/rebase merges (the dominant workflow) produce new SHAs so `is_ancestor` returns false anyway; the merged path only fires for regular/ff merges which require pushing first. Branches created with `wt new` that haven't diverged from main are false positives — if the worktree is clean, the branch and worktree get deleted. Fix: gate the `merged` determination on `upstream_remote(branch).is_some()`. Unpushed ancestor branches should emit `skipping {branch} (no upstream)` rather than being silently excluded — consistent with how dirty/locked/cwd skips are surfaced. Same fix needed for the locked-worktree advisory message (line 383). One downside: branches where tracking was manually removed (`git branch --unset-upstream`) or pushed without `-u` won't auto-prune — acceptable since both are rare and `wt rm` works. Related: `is_dirty` uses `--untracked-files=normal` which doesn't see gitignored files — a worktree with only ignored files (`.env`, `target/`, `node_modules/`) appears clean. Consider adding `--ignored` to the dirty check as defense-in-depth.

## Code quality

- **Re-validate paths in `auto_link()`** (P2, S) — `link.rs:auto_link()` creates symlinks from paths loaded from `~/.wt/config` without calling `validate_path()`. CLI-provided paths are validated before entering config, but a hand-edited config with `../` paths would bypass validation. Defense-in-depth: call `validate_path()` in the `auto_link()` loop body.
- **`remove_dest` should refuse to delete directories** (P2, S) — `link.rs:remove_dest()` calls `remove_dir_all` when the destination is a real directory. With `--force`, a file-to-directory change across branches could cause silent recursive deletion of an entire directory tree. `remove_dest` should error on real directories instead of deleting them — it only needs to handle files and symlinks.

## Documentation

- **Shell completion install instructions** (P1, S) — `wt init <shell>` works but docs don't explain where to add the `eval` line per shell, or troubleshoot if completions don't load.
