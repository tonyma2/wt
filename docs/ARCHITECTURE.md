# Architecture

`wt` is a single-binary CLI that manages git worktrees under `~/.wt/worktrees/<id>/<repo>/`. It can also clone repositories as bare repos under `~/.wt/repos/<id>/<repo>/`.

## Module Graph

```
main.rs                 Entry point: parse CLI, dispatch to command, handle errors
├── cli.rs              Clap derive structs (Cli, Command). Only file with /// doc comments
├── commands.rs         Declares all subcommand modules (pub mod)
├── commands/
│   ├── clone.rs        `git init --bare` + add origin + fetch, then create the first worktree
│   ├── list.rs         Tabular worktree listing; columns size to content, shrinking to the terminal width
│   ├── rm.rs           Remove worktrees + branches, with multi-target and path resolution
│   ├── prune.rs        Global prune: merged (incl. squash-merged) branches, orphan report
│   ├── switch.rs       Get-or-create worktree: existing worktree, existing branch, or new branch (from HEAD or [base]) with fuzzy typo detection
│   ├── link.rs         Link store in <git-common-dir>/wt/links/: move files in, symlink them into every worktree; auto_link()
│   ├── init.rs         Shell integration: completions + auto-cd wrapper (dynamic branch completion for zsh, bash, fish)
│   └── tui.rs          Interactive picker — one list of worktrees grouped by repo, with fuzzy filter
├── tui.rs              Ratatui terminal setup/teardown (inline viewport, raw mode, panic hook)
├── fuzzy.rs            Levenshtein distance + close-match detection + subsequence scoring for TUI filter
├── git.rs              Git abstraction — all subprocess calls go through Git struct
├── layout.rs           ~/.wt filesystem layout: roots, path/cwd helpers, destination reserve/cleanup, managed worktree discovery
├── worktree.rs         Worktree/Status types + porcelain parsers + query helpers + shared parallel loader (load_all)
└── terminal.rs         TTY/color detection, stderr color support, terminal width (COLUMNS env, crossterm window size, then 132), display-width text helpers (display_width/pad/trunc/trunc_tail) — measure user text with these, not chars().count() or {:<w$}
```

## Key Types

**`Git`** (`git.rs`) — Wraps a repo path. Every method spawns `git -C <repo> ...` through three helpers: `run` (captured stderr → `git_err` message), `query` (trimmed stdout as `Option`), and `succeeds` (exit status as `bool`); network commands use `run_streaming`. `Git::find_repo(path: Option<&Path>)` is the static entry point used by every command to locate the repo: the worktree toplevel, or the git dir itself for a bare repo. `Git::worktree_status()`, `Git::tracks()`, and `Git::is_ignored()` are associated functions that run against a worktree path, not the admin repo (see [decisions.md](decisions.md)).

**`Worktree`** (`worktree.rs`) — Parsed from `git worktree list --porcelain`. Fields: `path`, `branch` (Option), `bare`, `detached`, `locked`, `prunable`. Bool fields have no `is_` prefix. Query helpers on `&[Worktree]`: `resolve_worktree()` (returns `Ok(None)` when not found, `Err` when ambiguous), `find_by_path()`, `branch_checked_out_elsewhere()`. `not_found_error()` builds the shared "did you mean" error (or explains a deleted worktree directory). `display_order()` is the one ordering for human-facing views (`list` table, `tui`): `main`/`master` first, then branches by name, then detached. `label()` names a worktree in messages: its quoted branch, or its `~` path when detached.

**`RepoInfo` / `WorktreeInfo` / `Status`** (`worktree.rs`) — `Status::of()` is the single dirty/ahead/behind check (also used by `rm` and `prune`). `WorktreeInfo` wraps a `Worktree` with its `Status` (`dirty`, `ahead`, `behind`; `summary()` renders `* ↑1 ↓2`) and a `current` flag set by `mark_current()`. `RepoInfo` groups worktrees by repo name. `load_all()` returns `Vec<RepoInfo>` — the canonical data model for multi-repo views consumed by `list --all` and `tui`.

**`Cli` / `Command`** (`cli.rs`) — Clap derive types. `Command` is a flat enum with one variant per subcommand. `///` doc comments become `--help` text via clap — this is the only file that uses doc comments.

## Data Flow

```
commands/                    Formatting, display, user interaction
  │
  │  single-repo: find_repo → Git → list_worktrees → parse_porcelain → query → act
  │  multi-repo:  load_all() → Vec<RepoInfo> → format/render
  │
worktree.rs                  Data loading, parsing, queries
  │  load_all()              parallel loader for list --all, tui (one thread per repo, nested per-worktree threads)
  │  parse_porcelain()       git porcelain → Vec<Worktree>
  │  enrich_worktrees()      parallel Status per worktree (one thread per worktree)
  │  mark_current()          flags the deepest worktree containing cwd (single repo or across repos)
  │
layout.rs                    Where things live on disk
  │  managed_worktrees()     filesystem walk → (worktree dir, gitdir or None if unparseable); admin_repos() maps them to admin repos
  │  create_*_dest()         reserve a fresh ~/.wt/<root>/<id>/<repo>/ directory
  │
git.rs                       Subprocess execution, no business logic
     list_worktrees()  worktree_status()  find_repo()  (raw output; parsing lives in worktree.rs)
```

Most commands follow the single-repo path: `find_repo → list_worktrees → parse_porcelain → query → act`. Multi-repo commands (`list --all`, `tui`) share `load_all()`. If a new command needs all-repo status data, it should consume `load_all()`, not reimplement the pipeline.

### Exceptions and non-obvious behaviors

- **clone** — never calls `find_repo()`; `git init --bare` under `~/.wt/repos/`, adds `origin`, fetches once, creates only the default branch (tracking `origin`), then the first worktree (see [decisions.md](decisions.md))
- **switch** — a branch that exists only on one remote becomes a local branch tracking it (`Start::NewBranch(refs/remotes/<remote>/<name>)`) rather than relying on git's checkout guessing, so it reports the upstream. A `[base]` always creates a new branch and skips the typo check. Every path that creates ends in `create_worktree()`: prune stale metadata if a prunable entry holds the branch (it would block `git worktree add`), reserve a destination, `add_worktree()` or `checkout_worktree()`, clean up on failure, auto-link, print the path.
- **link / auto-link** — shared files live in the link store (`<git-common-dir>/wt/links/`), the same for every worktree of a repo; no worktree is the source, and the files in the store are the list of what is shared (see [decisions.md](decisions.md))
- **rm** — `resolve_target()` falls back from branch name to filesystem path. Also works without a repo context if given a path directly (resolves the admin repo from the worktree's `.git` file)
- **prune** — walks `managed_worktrees()` directly, not `load_all()`, because it needs `Git` handles for prune operations rather than precomputed status. Per repo, if any worktree is a candidate, it fetches `origin` (also under `--dry-run`, so a dry run reports exactly what a real run would remove) and removes worktrees whose branch is pushed and merged into the default branch. Afterwards it reports orphans under `~/.wt/worktrees/`. Safety rules are in [decisions.md](decisions.md). Output streams per repo group
- **tui** — consumes `load_all()`, maps `WorktreeInfo` → display structs (sorted by `display_order()`), filters bare and prunable, and flattens every repo into one list grouped by repo. Rows compare repos by index, so same-named repos stay separate groups. Typing ranks repos by their best match and keeps rows grouped; Enter always picks the highlighted row. The picker owns its scroll offset: jumps (opening, ←/→, clearing the filter) put the cursor's repo at the top, arrow moves scroll minimally, and the repo label sticks to the top visible row. `columns()` gives way in a fixed order as the terminal narrows (see [decisions.md](decisions.md)). Default command when no subcommand is given. The selection is written to `$__WT_CD` when the shell wrapper set it, otherwise printed to stdout. Coupling is one-way: `commands/tui.rs` depends on `worktree`, `layout`, `fuzzy`, `terminal`, and `tui` — none of those modules reference the TUI. Shared logic belongs in the dependency, not the consumer
- **init** — patches clap_complete's generated script via string replacement (the `COMPLETERS` table) to inject custom zsh completion functions. Fragile: replacement targets are `///` doc comments on `name`/`names`/`base`/`files` args in `cli.rs` — each must be unique per subcommand. `render()` fails if any target is missing; the `zsh_wires_each_completer_into_its_subcommands` unit test also catches targets that collide. Bash and fish get a smaller hand-written completer (`BASH_COMPLETER`, `FISH_COMPLETER`) for branch operands; bash's is registered by replacing clap's `complete -F _wt` line. Completion behavior is covered by the `zsh_*` and `bash_*` tests in `tests/init.rs`, which skip when zsh is not installed (see [decisions.md](decisions.md))

## Filesystem Layout

```
~/.wt/
├── repos/                  Bare repos created by `wt clone`
│   └── <random-id>/
│       └── <repo-name>/    Bare git repository (shared files in wt/links/)
└── worktrees/
    └── <random-id>/        6-char hex (e.g. a3f2b1)
        └── <repo-name>/    Worktree directory (created by git)
```

For repos added via `git clone` (the traditional workflow), the admin repo lives wherever the user cloned it. For repos added via `wt clone`, the admin repo is a bare clone under `~/.wt/repos/`. In both cases, worktree directories contain a `.git` file (not a directory) pointing back to `worktrees/<name>` in the admin repo.
