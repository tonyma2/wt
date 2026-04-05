# Design Decisions

Intentional choices that look like they could be "improved." Read the rationale before changing any of these.

## Do not add error crates

No `anyhow`, `thiserror`, or custom error enums. Every error is a `String` that exits 1. Typed error recovery adds complexity with no benefit — the binary never catches and branches on error variants.

## Do not "fix" `is_dirty()` to match other `Git` methods

All `Git` methods run `git -C <admin_repo>`. `is_dirty()` runs `git -C <worktree_path>` instead. This is correct: `git status` reports the tree it's pointed at, so running it against the admin repo silently gives the wrong dirty status.

## Do not revert `wt new` to auto-create branches

`wt new` originally auto-created branches when they didn't exist. Typos silently created branches instead of erroring. The explicit `-c`/`--create` flag was a deliberate fix.

## Do not print human-readable messages to stdout

Shell wrappers depend on `cd "$(wt new ...)"` capturing stdout as a path. Any non-data output on stdout breaks downstream consumers. Status messages go to stderr.

## Do not replace string replacement with a custom clap completer

Zsh completions inject custom functions via string replacement on clap_complete's generated script. A custom clap completer would pull in more of clap's internals for the same result. Keep the complexity in shell, not Rust.

**Caveat:** the replacement strings are coupled to the `///` help text on `name`, `names`, `base`, and `files` args in `cli.rs`. Each subcommand's `name` arg needs a unique doc comment so the replacement targets don't collide (`"Branch name, tag, or ref"` for path, `"Branch name"` for switch, `"Branch name or ref"` for new). Changing those docstrings silently degrades completion. The unit tests `zsh_completion_is_dynamic` and `zsh_link_unlink_completions_are_dynamic` catch this — run them after editing `cli.rs` arg help text.

## Do not add doc comments outside `cli.rs`

`///` doc comments in `cli.rs` serve a concrete purpose: clap derives `--help` text from them on structs that derive `Parser`/`Subcommand`. Everywhere else they have no mechanical effect — they'd just be prose attached to internal functions in a binary that has no public API consumers. Code is self-documenting; inline `//` comments explain non-obvious *why*, not *what*.

## Do not split config into per-repo or in-repo files

Link persistence uses `~/.wt/config` with repo paths as TOML keys. Alternatives considered: `.wtlinks` in the repo (pollutes the repository), git config (wrong abstraction for file lists), per-repo config files under `~/.wt/` (more filesystem complexity). A single file is simple to read, edit, and back up.

## Do not require `-c` for new branches in `switch`

`wt new` requires `-c` to create a branch (see above). `wt switch` is intentionally more lenient — its purpose is "get me into this branch, fast." Requiring `-c` for every new branch would negate the convenience. Instead, `switch` uses Levenshtein distance to detect likely typos and suggests the close match. `-c` bypasses the fuzzy check when the user genuinely wants a new similarly-named branch.

## Do not add SHA prefix matching for detached HEAD lookup

`wt path` and `wt rm` resolve names as: branch → ref (tag/SHA) → path. The ref fallback only matches detached HEAD worktrees whose `head` SHA equals the resolved ref. Arbitrary SHA prefix matching is intentionally excluded because git worktrees can share a HEAD commit, making prefix matches ambiguous.

## Do not change cd hint TTY detection to check stderr

After creating a worktree, `new` and `switch` print a `cd "$(wt path '...')"` hint to **stderr** — but only when **stdout** is a TTY.

- In `cd "$(wt new ...)"`, stdout is captured by the command substitution while stderr remains attached to the terminal
- Checking stdout-is-TTY (not stderr-is-TTY) correctly suppresses the hint for wrapper users (stdout piped) and shows it for bare invocations (stdout is the terminal)
- Branch name is single-quoted to prevent shell expansion of `$`, `` ` ``, and `"` characters
- When `wt init` wrapper is active, stdout is captured by `$()`, so `is_stdout_tty()` returns false and the hint is naturally suppressed

## Do not use stdout capture for TUI cd-back

Subcommands like `new` and `switch` return paths via stdout, captured by the shell wrapper with `out=$(command wt ...)`. The TUI picker can't do this — it renders to stdout. Instead, the zero-arg wrapper creates a temp file with `mktemp`, passes its path via `__WT_CD`, and the binary writes the selected path there. The shell reads it back after the process exits. Any new subcommand that uses both stdout rendering and cd-back must use the temp-file mechanism, not stdout capture.

## Do not store bare repos in the current directory

`wt clone` stores bare repos under `~/.wt/repos/<id>/<name>/`, not in the current directory like `git clone`. Users never interact with the bare repo directly — they work inside worktrees. Hiding the bare repo avoids the "where did I put that repo" problem and keeps the filesystem clean. The random id prevents collisions when cloning repos with the same name from different orgs.

## Do not use `worktrees.first()` as primary worktree

`link`, `unlink`, and `switch` need the "primary" worktree (the source for symlinks). The primary is identified by matching `repo_root` (from `find_repo`) against worktree paths, with canonical path comparison. Fallback: first non-bare entry. This is deterministic because `repo_root` is always the worktree you're operating from.

Previous approaches failed: `.first()` returns the bare entry for bare repos. `.find(|wt| !wt.bare)` is non-deterministic because `git worktree list` orders linked worktrees by `readdir()` over the internal `worktrees/` directory, which varies by filesystem. Similarly, `skip(1)` to get "linked" worktrees is wrong for bare repos — filter by path instead.

## Do not mock git in tests

Tests run the real binary against real temp repos. Mocks hide the git version differences and filesystem edge cases that matter most for a tool that wraps git. Slower tests are acceptable at this codebase size.
