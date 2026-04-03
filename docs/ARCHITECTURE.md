# Architecture

`wt` is a single-binary CLI that manages git worktrees under `~/.wt/worktrees/<id>/<repo>/`. It can also clone repositories as bare repos under `~/.wt/repos/<id>/<repo>/`.

## Module Graph

```
main.rs                 Entry point: parse CLI, dispatch to command, handle errors
├── cli.rs              Clap derive structs (Cli, Command). Only file with /// doc comments
├── commands.rs         Declares all subcommand modules (pub mod)
├── commands/
│   ├── clone.rs        Bare-clone a repo + create first worktree + fix fetch refspec
│   ├── new.rs          Create worktree (checkout existing ref or create branch)
│   ├── list.rs         Tabular worktree listing with terminal-aware column sizing
│   ├── rm.rs           Remove worktrees + branches, with multi-target and path resolution
│   ├── prune.rs        Global prune: stale metadata, merged branches, orphaned directories
│   ├── path.rs         Print worktree path by branch name or ref
│   ├── switch.rs       Get-or-create worktree with fuzzy typo detection
│   ├── link.rs         Symlink files from primary worktree into all linked worktrees
│   ├── unlink.rs       Remove symlinks created by link from all linked worktrees
│   ├── init.rs         Shell integration: completions + auto-cd wrapper (zsh gets dynamic branch completion)
│   └── tui.rs          Interactive picker — two-pane repo/worktree browser with fuzzy filter
├── tui.rs              Ratatui terminal setup/teardown (inline viewport, raw mode, panic hook)
├── config.rs           Read/write ~/.wt/config TOML (auto-link persistence)
├── fuzzy.rs            Levenshtein distance + close-match detection + subsequence scoring for TUI filter
├── git.rs              Git abstraction — all subprocess calls go through Git struct
├── worktree.rs         Worktree type + porcelain parser + query helpers + shared parallel loader (load_all)
└── terminal.rs         TTY/color detection, stderr color support, terminal width (COLUMNS env, ioctl fallback, then 132)
```

## Key Types

**`Git`** (`git.rs`) — Wraps a repo path. Every method spawns `git -C <repo> ...` and returns `Result<T, String>` or `bool`. `Git::find_repo(path: Option<&Path>)` is the static entry point used by every command to locate the admin repo. Exceptions: `is_dirty()` and `worktree_status()` run against the worktree path, not the admin repo (see [decisions.md](decisions.md)).

**`Worktree`** (`worktree.rs`) — Parsed from `git worktree list --porcelain`. Fields: `path`, `head`, `branch` (Option), `bare`, `detached`, `locked`, `prunable`. Bool fields have no `is_` prefix. Query helpers on `&[Worktree]`: `find_live_by_branch()`, `find_live_by_head()`, `find_by_path()`, `branch_checked_out_elsewhere()`.

**`RepoInfo` / `WorktreeInfo`** (`worktree.rs`) — Returned by `load_all()`. `WorktreeInfo` mirrors `Worktree` fields and adds computed status: `dirty`, `ahead`, `behind`, `current`. `RepoInfo` groups worktrees by repo name. This is the canonical data model for multi-repo views — both `list --all` and `tui` consume it.

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
  │  discover_repos()        filesystem walk — used by load_all and prune independently
  │  parse_porcelain()       git porcelain → Vec<Worktree>
  │  enrich_worktrees()      parallel dirty + ahead/behind (one thread per worktree)
  │
git.rs                       Subprocess execution, no business logic
     list_worktrees()  worktree_status()  is_dirty()  find_repo()
```

Most commands follow the single-repo path: `find_repo → list_worktrees → parse_porcelain → query → act`. Multi-repo commands (`list --all`, `tui`) share `load_all()`. If a new command needs all-repo status data, it should consume `load_all()`, not reimplement the pipeline.

### Exceptions and non-obvious behaviors

- **clone** — never calls `find_repo()`; creates its own bare repo, fixes the fetch refspec (`+refs/heads/*:refs/remotes/origin/*`), fetches, then creates the first worktree. Bare repo stored under `~/.wt/repos/`
- **new** — never lists worktrees; builds a destination path directly and calls `add_worktree()` or `checkout_worktree()`
- **rm** — `resolve_target()` has a three-stage fallback: branch → ref-to-SHA → filesystem path. Also works without a repo context if given a path directly (resolves the admin repo from the worktree's `.git` file)
- **prune** — global mode (no `--repo`) uses `discover_repos()` directly, not `load_all()`, because it needs `Git` handles for prune operations rather than precomputed status
- **switch** — auto-prunes stale worktree metadata when it encounters a prunable match before creating
- **tui** — consumes `load_all()`, maps `WorktreeInfo` → display structs, filters bare. Default command when no subcommand is given. Coupling is one-way: `commands/tui.rs` depends on `worktree`, `fuzzy`, `terminal`, and `tui` — none of those modules reference the TUI. Shared logic belongs in the dependency, not the consumer
- **init** — patches clap_complete's generated script via string replacement to inject custom zsh completion functions. Fragile: replacement targets are `///` doc comments on `name`/`names`/`base`/`files` args in `cli.rs` — each must be unique per subcommand. Guarded by the `zsh_completion_is_dynamic` and `zsh_link_unlink_completions_are_dynamic` unit tests (see [decisions.md](decisions.md))

## Filesystem Layout

```
~/.wt/
├── config                  TOML config (auto-link file list per repo)
├── repos/                  Bare repos created by `wt clone`
│   └── <random-id>/
│       └── <repo-name>/    Bare git repository
└── worktrees/
    └── <random-id>/        6-char hex (e.g. a3f2b1)
        └── <repo-name>/    Worktree directory (created by git)
```

For repos added via `git clone` (the traditional workflow), the admin repo lives wherever the user cloned it. For repos added via `wt clone`, the admin repo is a bare clone under `~/.wt/repos/`. In both cases, worktree directories contain a `.git` file (not a directory) pointing back to `worktrees/<name>` in the admin repo.
