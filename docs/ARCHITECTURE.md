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
│   ├── path.rs         Print worktree path by branch name
│   ├── link.rs         Symlink files from primary worktree into all linked worktrees
│   └── completions.rs  Generate shell completions (zsh gets dynamic branch completion)
├── git.rs              Git abstraction — all subprocess calls go through Git struct
├── worktree.rs         Worktree type + porcelain parser + query helpers
└── terminal.rs         Terminal width detection (COLUMNS env, ioctl fallback, then 132)
```

## Key Types

**`Git`** (`git.rs`) — Wraps a repo path. Every method spawns `git -C <repo> ...` and returns `Result<T, String>` or `bool`. `Git::find_repo(path: Option<&Path>)` is the static entry point used by every command to locate the admin repo. Exception: `is_dirty()` runs against the worktree path, not the admin repo (see [decisions.md](decisions.md)).

**`Worktree`** (`worktree.rs`) — Parsed from `git worktree list --porcelain`. Fields: `path`, `head`, `branch` (Option), `bare`, `detached`, `locked`, `prunable`. Bool fields have no `is_` prefix. Query helpers on `&[Worktree]`: `find_by_branch()`, `find_by_path()`, `branch_checked_out_elsewhere()`.

**`Cli` / `Command`** (`cli.rs`) — Clap derive types. `Command` is a flat enum with one variant per subcommand. `///` doc comments become `--help` text via clap — this is the only file that uses doc comments.

## Data Flow

Commands that query existing worktrees (list, rm, path, link, prune) follow this pattern:

```
Git::find_repo(repo_arg)  →  Git::new(repo_root)  →  git.list_worktrees()
    →  worktree::parse_porcelain()  →  query Vec<Worktree>  →  act
```

Exceptions:
- **new** never lists worktrees — it builds a destination path and calls `add_worktree()` or `checkout_worktree()` directly.
- **prune** (global, no `--repo`) discovers repos by walking the filesystem and parsing `.git` files, not via `list_worktrees()`.

## Commands

**new** — Resolves repo, builds destination path as `~/.wt/worktrees/<random-id>/<repo>`. The 6-char hex random ID ensures unique paths regardless of branch name. Validates (with `-c`) that the branch doesn't already exist. Creates a new branch (`git worktree add -b`) or checks out an existing ref (`git worktree add`). Prints the path to stdout.

**list** — `--porcelain` passes git output through unchanged. Human mode calculates column widths from terminal width, queries dirty/ahead-behind status per worktree, formats a table, and marks the current worktree with `*`.

**rm** — Accepts branch names or absolute paths. `resolve_target()` tries branch lookup first, falls back to path resolution.
- Validates: not primary worktree, not cwd, branch exists locally, not checked out elsewhere, not dirty/unmerged (unless `--force`)
- Removes worktree, then deletes the branch
- Multiple targets accumulate errors rather than aborting on the first

**prune** — Two modes:
- With `--repo`: prunes a single repo's stale metadata and merged worktrees
- Without `--repo` (default): discovers all repos from `~/.wt/worktrees/` via recursive `.git` file parsing, prunes each, then finds orphaned directories and cleans up empty parents
- Merged-worktree pruning skips dirty worktrees; skipped entirely if no remote exists. `--base` overrides the auto-detected default branch for merged detection
- `--gone` removes worktrees whose upstream is gone (fetches each unique remote once, skipped in `--dry-run`)

**path** — Looks up branch in parsed worktree list, prints its path to stdout. Errors on ambiguous matches.

**link** — Validates relative paths (no `..`, not absolute). Checks source files exist in the primary worktree before touching any linked worktree. Creates symlinks (and intermediate directories) pointing back to the primary worktree's copy. Skips correct existing links, warns on conflicts unless `--force`.

**completions** — Generates clap_complete output. For zsh, injects four custom functions (`_wt_collect_worktree_rows`, `_wt_complete_branches_with_paths`, `_wt_path_branches`, `_wt_remove_targets`) and patches the generated script via string replacement.

## Filesystem Layout

```
~/.wt/worktrees/
└── <random-id>/            6-char hex (e.g. a3f2b1)
    └── <repo-name>/        Worktree directory (created by git)
```

The admin (primary) repo lives wherever the user cloned it. Worktree directories contain a `.git` file (not a directory) pointing back to `.git/worktrees/<name>` in the admin repo.

## Test Harness

Integration tests live in `tests/` with one file per subcommand. Shared helpers are in `tests/common/mod.rs`.

### Setup

**`setup()`** — Creates a `TempDir` with a git repo in a `repo/` subdirectory (single empty commit on `main`). Returns `(TempDir, repo_path)`. `HOME` is set to the `TempDir` root so worktrees land at `<TempDir>/.wt/worktrees/<id>/repo/`. Keep `TempDir` in scope — dropping it deletes the directory.

**`setup_with_origin()`** — Like `setup()` but creates a bare remote as `origin`, pushes `main`, and fetches to set up tracking refs. Returns `(TempDir, repo_path, origin_path)`.

**`init_repo(dir)` / `init_bare_repo(dir)`** — Initialize a regular or bare repo. Available standalone for tests that need additional repos.

### Running

**`wt_bin()`** — Returns a `Command` for the compiled binary.

**`wt(home)`** — Returns a `Command` for the compiled binary with `HOME` set.

**`run_wt(home, configure)`** — Closure-based `wt` runner with consistent `HOME` setup that returns `Output`.

**`wt_new(home, repo, branch)`** — Runs `wt new -c <branch> --repo <repo>` with `HOME` overridden. Returns the created worktree path.

**`wt_checkout(home, repo, name)`** — Same but without `-c` (checks out existing ref).

**`git(dir)`** — Returns a `Command` for `git -C <dir>`.

### Assertions

**`assert_git_success(dir, args)`** — Runs a git command and panics if it fails.

**`assert_git_success_with(dir, configure)`** — Closure variant for commands needing extra args or env.

**`assert_git_stdout_success(dir, args)`** — Runs a git command, panics if it fails, returns stdout.

**`assert_branch_absent(dir, branch)`** — Panics if the branch exists.

**`assert_branch_present(dir, branch)`** — Panics if the branch does not exist.

**`assert_exit_code(output, code)`** — Panics unless the process exited with exactly `code`.

**`assert_stdout_empty(output)`** — Panics unless stdout is empty.

**`assert_stderr_empty(output)`** — Panics unless stderr is empty.

**`assert_stderr_exact(output, expected)`** — Panics unless stderr exactly matches `expected`.

**`assert_error(output, code, expected_stderr)`** — Composite assertion for error contracts:
exit code + empty stdout + exact stderr.

**`canonical(path)`** — Canonicalizes a path, falling back to the original when canonicalization fails.

**`normalize_home_paths(output, home)`** — Replaces canonical and raw home prefixes with `$HOME` for stable output assertions.
