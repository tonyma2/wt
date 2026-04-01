# Backlog

**P1** now · **P2** later · **P3** maybe

**S** quick · **M** hours · **L** days

Delete when shipped. New entries: `- **title** (P#, S/M/L) — description`

## Pruning

- **Prune no-upstream worktrees** (P2, M) — branches that were never pushed (no tracking upstream). Catches worktrees created for exploration that saw no use. Needs a staleness heuristic (e.g. last commit age) to avoid pruning active local-only work.

## Clone

- **Pass through clone progress** (P2, S) — `bare_clone` uses `--quiet` and `Stdio::null()`, swallowing git progress. Drop `--quiet` and inherit stderr so large clones show progress without breaking the stdout-is-data contract.

## Performance

- **Parallelize `wt list` status checks** (P2, S) — `computed_status` shells out twice per worktree sequentially. Precompute with `thread::scope` like TUI `load_repos`. Covers single-repo and `--all` paths.

