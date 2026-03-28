# Backlog

**P1** now · **P2** later · **P3** maybe

**S** quick · **M** hours · **L** days

Delete when shipped. New entries: `- **title** (P#, S/M/L) — description`

## Bugs

- **`find_by_path` skips canonicalization** (P2, S) — `worktree::find_by_path` and `branch_checked_out_elsewhere` compare `wt.path` (from git porcelain, not canonical) against caller-supplied paths (often canonical). Fails when symlinks are involved (e.g. `/tmp` → `/private/tmp`). Fix: canonicalize both sides in `find_by_path`.
- **Config temp file name is fixed** (P3, S) — `config::save` writes to `~/.wt/config.tmp` then renames. Two concurrent `wt` processes clobber each other's temp file. Fix: use a unique temp name (PID or random suffix).

## Features

- **TUI repo/worktree picker** (P2, M) — interactive picker to jump between repos and worktrees across all managed projects. Complements `wt clone` by giving users a way back into bare-cloned repos.
