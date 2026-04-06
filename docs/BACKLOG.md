# Backlog

**P1** now · **P2** later · **P3** maybe

**S** quick · **M** hours · **L** days

Delete when shipped. New entries: `- **title** (P#, S/M/L) — description`

## Pruning

- **Prune no-upstream worktrees** (P2, M) — branches that were never pushed (no tracking upstream). Catches worktrees created for exploration that saw no use. Needs a staleness heuristic (e.g. last commit age) to avoid pruning active local-only work.

## Code organization

- **Audit function placement and subprocess consolidation** (P2, M) — sweep the codebase for functions that ended up in the wrong place or could be merged. Patterns to look for: multiple git subprocess calls that retrieve data available from a single command, methods on `&self` that never use `self`, thin wrappers that just glue two calls and obscure what's really happening, and related logic split across modules without a clear reason. Prior art: `is_dirty` + `ahead_behind` (two subprocesses) became a single `worktree_status` parsing `git status --porcelain=v2 --branch`.
- **Rethink `load_all` / `load_all_from` split** (P3, S) — `load_all` is a one-line wrapper over `load_all_from` that exists only so tests can inject a temp dir instead of `worktrees_root()`. Simplest fix may be to drop the wrapper and have callers pass the root, but needs more thought — consider whether there's a cleaner testability pattern that doesn't add a wrapper per entry point.
