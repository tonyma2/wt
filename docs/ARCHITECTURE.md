# Architecture

`wt` is a single-binary CLI that manages git worktrees under `~/.wt/worktrees/<id>/<repo>/`.

## Module Graph

```
main.rs                 Entry point: parse CLI, dispatch to command, handle errors
├── cli.rs              Clap derive structs (Cli, Command). Only file with /// doc comments
├── commands.rs         Declares all subcommand modules (pub mod)
├── commands/
│   ├── new.rs          Create worktree (checkout existing ref or create branch)
│   ├── list.rs         Tabular worktree listing with terminal-aware column sizing
│   ├── rm.rs           Remove worktrees + branches, with multi-target and path resolution
│   ├── prune.rs        Global prune: stale metadata, merged branches, orphaned directories
│   ├── path.rs         Print worktree path by branch name or ref
│   ├── switch.rs       Get-or-create worktree with fuzzy typo detection
│   ├── link.rs         Symlink files from primary worktree into all linked worktrees
│   ├── unlink.rs       Remove symlinks created by link from all linked worktrees
│   └── init.rs         Shell integration: completions + auto-cd wrapper (zsh gets dynamic branch completion)
├── config.rs           Read/write ~/.wt/config TOML (auto-link persistence)
├── fuzzy.rs            Levenshtein distance + close-match detection for typo prevention
├── git.rs              Git abstraction — all subprocess calls go through Git struct
├── worktree.rs         Worktree type + porcelain parser + query helpers
└── terminal.rs         TTY/color detection, stderr color support, terminal width (COLUMNS env, ioctl fallback, then 132)
```

## Key Types

**`Git`** (`git.rs`) — Wraps a repo path. Every method spawns `git -C <repo> ...` and returns `Result<T, String>` or `bool`. `Git::find_repo(path: Option<&Path>)` is the static entry point used by every command to locate the admin repo. Exception: `is_dirty()` runs against the worktree path, not the admin repo (see [decisions.md](decisions.md)).

**`Worktree`** (`worktree.rs`) — Parsed from `git worktree list --porcelain`. Fields: `path`, `head`, `branch` (Option), `bare`, `detached`, `locked`, `prunable`. Bool fields have no `is_` prefix. Query helpers on `&[Worktree]`: `find_live_by_branch()`, `find_live_by_head()`, `find_by_path()`, `branch_checked_out_elsewhere()`.

**`Cli` / `Command`** (`cli.rs`) — Clap derive types. `Command` is a flat enum with one variant per subcommand. `///` doc comments become `--help` text via clap — this is the only file that uses doc comments.

## Data Flow

Commands that query existing worktrees (list, rm, path, switch, link, unlink, prune) follow this pattern:

```
Git::find_repo(repo_arg)  →  Git::new(repo_root)  →  git.list_worktrees()
    →  worktree::parse_porcelain()  →  query Vec<Worktree>  →  act
```

Exceptions and non-obvious behaviors:
- **new** — never lists worktrees; builds a destination path directly and calls `add_worktree()` or `checkout_worktree()`
- **rm** — `resolve_target()` has a three-stage fallback: branch → ref-to-SHA → filesystem path. Also works without a repo context if given a path directly (resolves the admin repo from the worktree's `.git` file)
- **prune** — global mode (no `--repo`) uses a completely different data flow: walks `~/.wt/worktrees/` recursively, parses `.git` files to discover admin repos, then prunes each
- **switch** — auto-prunes stale worktree metadata when it encounters a prunable match before creating
- **init** — patches clap_complete's generated script via string replacement to inject custom zsh completion functions. Fragile: replacement targets are `///` doc comments on `name`/`names`/`base` args in `cli.rs` — each must be unique per subcommand. Guarded by the `zsh_completion_is_dynamic` unit test (see [decisions.md](decisions.md))

## Filesystem Layout

```
~/.wt/
├── config                  TOML config (auto-link file list per repo)
└── worktrees/
    └── <random-id>/        6-char hex (e.g. a3f2b1)
        └── <repo-name>/    Worktree directory (created by git)
```

The admin (primary) repo lives wherever the user cloned it. Worktree directories contain a `.git` file (not a directory) pointing back to `.git/worktrees/<name>` in the admin repo.
