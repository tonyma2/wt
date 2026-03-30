# Backlog

**P1** now · **P2** later · **P3** maybe

**S** quick · **M** hours · **L** days

Delete when shipped. New entries: `- **title** (P#, S/M/L) — description`

## Performance

- **Parallelize `wt list` status checks** (P2, S) — `computed_status` shells out twice per worktree sequentially. Precompute with `thread::scope` like TUI `load_repos`. Covers single-repo and `--all` paths.

