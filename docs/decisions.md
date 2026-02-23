# Design Decisions

Intentional choices that look like they could be "improved." Do not change these without reading the rationale.

## Do not add error crates

No `anyhow`, `thiserror`, or custom error enums. Every error is a `String` that exits 1. Typed error recovery adds complexity with no benefit — the binary never catches and branches on error variants.

## Do not "fix" `is_dirty()` to match other `Git` methods

All `Git` methods run `git -C <admin_repo>`. `is_dirty()` runs `git -C <worktree_path>` instead. This is correct: `git status` reports the tree it's pointed at, so running it against the admin repo silently gives the wrong dirty status.

## Do not revert `wt new` to auto-create branches

`wt new` originally auto-created branches when they didn't exist. Typos silently created branches instead of erroring. The explicit `-c`/`--create` flag was a deliberate fix.

## Do not print human-readable messages to stdout

Shell wrappers depend on `cd "$(wt new ...)"` capturing stdout as a path. Any non-data output on stdout breaks downstream consumers. Status messages go to stderr.

## Do not replace string replacement with a custom clap completer

Zsh completions inject custom functions via string replacement on clap_complete's generated script. A custom clap completer would pull in more of clap's internals for the same result. Keep the complexity in shell, not Rust. **Caveat:** the replacement strings are coupled to the `///` help text on the `name` and `names` args in `cli.rs`. Changing those docstrings silently degrades completion. The unit test `zsh_path_completion_is_dynamic` catches this — run it after editing `cli.rs` arg help text.

## Do not add doc comments outside `cli.rs`

Clap derives `--help` text from `///` doc comments on CLI structs and fields. Adding doc comments elsewhere sets a false expectation that the codebase documents public APIs — it doesn't. Code is self-documenting; inline comments explain non-obvious *why*, not *what*.

## Config is per-repo keyed in a single global file

Link persistence uses `~/.wt/config` with repo paths as TOML keys. Alternatives considered: `.wtlinks` in the repo (pollutes the repository), git config (wrong abstraction for file lists), per-repo config files under `~/.wt/` (more filesystem complexity). A single file is simple to read, edit, and back up.

## Switch uses fuzzy matching instead of a `-c` gate

`wt new` requires `-c` to create a branch (see above). `wt switch` is intentionally more lenient — its purpose is "get me into this branch, fast." Requiring `-c` for every new branch would negate the convenience. Instead, `switch` uses Levenshtein distance to detect likely typos and suggests the close match. `-c` bypasses the fuzzy check when the user genuinely wants a new similarly-named branch.

## Do not mock git in tests

Tests run the real binary against real temp repos. Mocks hide the git version differences and filesystem edge cases that matter most for a tool that wraps git. Slower tests are acceptable at this codebase size.
