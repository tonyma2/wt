# Backlog

**P1** now · **P2** later · **P3** maybe

**S** quick · **M** hours · **L** days

Delete when shipped.

## Features

- **Post-create hooks** (P1, M) — config-driven commands (`npm install`, `cp .env`, etc.) run after worktree creation. Single `post-create` entry per repo in `~/.wt/config`, failures non-fatal. Makes `new`/`switch` produce ready-to-work worktrees.
- **`wt clone`** (P1, L) — `wt clone <url>` → bare clone + first worktree + correct fetch refspec. One opinionated flow, don't replicate `git clone`'s flag surface. Kills onboarding friction.
- **Global status view** (P1, M) — cross-repo worktree summary via `~/.wt/worktrees/` discovery (same mechanism as global `prune`). Could be `wt list --all` or a separate subcommand.
- **`wt lock/unlock`** (P2, S) — thin wrappers around `git worktree lock/unlock` addressed by branch name instead of path. Small win but closes the feature gap.
- **`wt run <branch> -- <cmd>`** (P2, M) — execute a command in a worktree's directory without cd. Useful for scripting and CI. Less critical with shell integration, since `(cd "$(wt path branch)" && cmd)` works.
- **Dynamic bash completions** (P2, L) — zsh gets custom branch completions with status badges via string replacement in `init.rs`. Bash only gets static clap_complete output (no branch names). Real gap, high effort.
- **Prune `--stats` summary** (P3, S) — add a summary line at the end of `wt prune` output: "3 removed, 1 skipped (dirty), 2 skipped (locked)". Currently each item is reported individually but there's no aggregate view.
- **`wt repair`** (P3, M) — fix broken worktree backpointers and `.git` file references. Global discovery mode (like `prune`) is the main value-add. Most issues are already handled by `prune`.
- **`--verbose` / `WT_DEBUG=1`** (P3, M) — log git commands (args + exit codes) and resolution steps to stderr. Useful for troubleshooting. No overhead when off.
- **Git subprocess timeouts** (P3, M) — a hanging `git fetch` blocks `wt prune --gone` forever. Add configurable timeout via `WT_GIT_TIMEOUT` env var. Rare in practice (Ctrl+C works).

## Code quality

- **Re-validate paths in `auto_link()`** (P2, S) — `link.rs:auto_link()` creates symlinks from paths loaded from `~/.wt/config` without calling `validate_path()`. CLI-provided paths are validated before entering config, but a hand-edited config with `../` paths would bypass validation. Defense-in-depth: call `validate_path()` in the `auto_link()` loop body.
- **Multi-target `rm` partial failure tests** (P1, S) — `rm.rs` accumulates errors across multiple targets, but no integration test covers partial failure. Add tests for mixed success/failure scenarios.

## Distribution

- **Publish to crates.io** (P1, S) — not yet published. Biggest single adoption unlock.
- **Pre-built binaries + Homebrew** (P1, L) — release profile is already optimized. Add CI workflows for cross-platform builds (macOS aarch64 + x86_64, Linux) and a Homebrew tap.
- **CHANGELOG.md** (P1, S) — rich conventional commit history exists but no user-facing changelog.
- **Multi-platform CI** (P2, M) — tests only run on Ubuntu. Add macOS (validates path canonicalization, terminal width) and optionally Windows.

## Documentation

- **Getting started guide** (P1, M) — visual demo or animated GIF showing the core workflow. The README is terse; a quick-start section with real-world examples would help adoption.
- **Shell completion install instructions** (P1, S) — `wt init <shell>` works but docs don't explain where to add the `eval` line per shell, or troubleshoot if completions don't load.
- **Ecosystem comparison in README** (P2, S) — "How is `wt` different from raw `git worktree`?" Brief table highlighting unique features.
- **CONTRIBUTING.md** (P2, M) — how to add a subcommand, test patterns, where config/state is stored, the completion system.
- **Issue and PR templates** (P2, S) — `.github/` templates for bug reports and PRs.
- **Man pages** (P3, S) — auto-generate from clap structs via `clap_mangen`.
