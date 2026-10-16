# Design Decisions

Intentional choices that look like they could be "improved." Read the rationale before changing any of these.

## Do not add error crates

No `anyhow`, `thiserror`, or custom error enums. Every error is a `String` that exits 1. Typed error recovery adds complexity with no benefit — the binary never catches and branches on error variants.

## Do not "fix" `worktree_status()` or `tracks()` to match other `Git` methods

All `Git` methods run `git -C <admin_repo>`. `worktree_status()` (behind `Status::of()`, the only dirty check) and `tracks()` (the tracked-file check in `link`) run `git -C <worktree_path>` instead. This is correct: `git status` and `git ls-files` report the tree and index they're pointed at, so running them against the admin repo silently gives the wrong answer (or fails, for a bare repo).

## Do not split `switch` into separate checkout and create commands

`wt new` (check out, `-c` to create) used to sit beside `wt switch` (get-or-create). Their errors pointed at each other, and the split existed only so typos couldn't create branches. `switch` guards against that with a Levenshtein check instead: a name close to an existing branch errors with a suggestion, and `-c` bypasses it when a similar name is intended. A `[base]` creates without the check, since naming a start point already states the intent. A name that exists only on a remote is checked out tracking it, never shadowed by a fresh branch from `HEAD`.

## Do not check out tags or SHAs as detached worktrees

`wt switch v1.0` errors and suggests `wt switch <branch> v1.0`. A detached worktree has no name to look it up by, so supporting them meant a second resolution path (ref → SHA → matching detached `HEAD`) in `path`, `rm`, and zsh completion. Starting a branch from the ref covers the use case; `git worktree add --detach` still works for a one-off, and `wt rm <path>` removes it. Worktrees that become detached (mid-rebase, `git checkout --detach`) are still listed and removable by path.

## Do not print human-readable messages to stdout

Shell wrappers depend on `cd "$(wt switch ...)"` capturing stdout as a path. Any non-data output on stdout breaks downstream consumers. Status messages go to stderr.

## Do not replace string replacement with a custom clap completer

Zsh completions inject custom functions via string replacement on clap_complete's generated script. A custom clap completer would pull in more of clap's internals for the same result. Keep the complexity in shell, not Rust.

**Caveat:** the replacement strings (the `COMPLETERS` table in `init.rs`) are coupled to the `///` help text on `name`, `names`, `base`, and `files` args in `cli.rs`. Each subcommand's `name` arg needs a unique doc comment so the replacement targets don't collide (`"Branch checked out in the worktree"` for path, `"Branch name"` for switch). Changing those docstrings silently degrades completion. `render()` errors when a target is missing, and the `zsh_wires_each_completer_into_its_subcommands` unit test catches collisions — run `cargo test init` after editing `cli.rs` arg help text.

## Do not add doc comments outside `cli.rs`

`///` doc comments in `cli.rs` serve a concrete purpose: clap derives `--help` text from them on structs that derive `Parser`/`Subcommand`. Everywhere else they have no mechanical effect — they'd just be prose attached to internal functions in a binary that has no public API consumers. Code is self-documenting; inline `//` comments explain non-obvious *why*, not *what*.

## Do not give shared files a source worktree

`wt link .env` moves the file into `<git-common-dir>/wt/links/` (listed in `wt/manifest`) and symlinks it into every worktree, including the one it came from. Previously one worktree held the real file and the others linked to it; removing that worktree (`wt rm`, or `git worktree remove`, which deletes gitignored files) destroyed the only copy and left every link dangling. With the store, no worktree is special: `rm` and `prune` need no link checks, and every worktree resolves the same store via `--git-common-dir`.

- The store lives with the repo, not in `~/.wt/`: a per-repo store under `~/.wt/` needs a repo → id mapping (config again) and leaks when the repo is deleted. Inside the git directory it never shows up in the working tree, isn't copied by `git clone`, and goes away with the repo. `prune` keeps a bare clone whose store still holds files
- Only untracked paths can be shared: a symlink over a tracked file shows as a change in every worktree
- `link` records the manifest entry before moving the file, so an interrupted run is completed by running `wt link` again
- `unlink` means "stop sharing", not "delete": each link becomes a copy before the store entry is removed, and if no worktree links to it the store copy is kept
- Symlinks are absolute: relative links break when only one side moves. Re-running `wt link` repairs them
- `~/.wt/config` (links keyed by source worktree) is migrated by `links::migrate_legacy_config()`, which `main` runs before every command so a source is moved into the store before `rm` or `prune` can delete it. Entries that fail stay in the file and are retried

## Do not change cd hint TTY detection to check stderr

After creating a worktree, `switch` and `clone` print a `cd '<path>'` hint to **stderr** — but only when **stdout** is a TTY.

- In `cd "$(wt switch ...)"`, stdout is captured by the command substitution while stderr remains attached to the terminal
- Checking stdout-is-TTY (not stderr-is-TTY) correctly suppresses the hint for wrapper users (stdout piped) and shows it for bare invocations (stdout is the terminal)
- The hint uses the literal path, not `wt path <name>`: a name lookup fails after `clone` (cwd is outside the repo) and with `--repo`. The path is single-quoted with `'` escaped as `'\''`
- When `wt init` wrapper is active, stdout is captured by `$()`, so `is_stdout_tty()` returns false and the hint is naturally suppressed

## Do not use stdout capture for TUI cd-back

Subcommands like `switch` and `clone` return paths via stdout, captured by the shell wrapper with `out=$(command wt ...)`. The TUI picker can't do this — it renders to stdout. Instead, the zero-arg wrapper creates a temp file with `mktemp`, passes its path via `__WT_CD`, and the binary writes the selected path there. The shell reads it back after the process exits. Any new subcommand that uses both stdout rendering and cd-back must use the temp-file mechanism, not stdout capture.

## Do not store bare repos in the current directory

`wt clone` stores bare repos under `~/.wt/repos/<id>/<name>/`, not in the current directory like `git clone`. Users never interact with the bare repo directly — they work inside worktrees. Hiding the bare repo avoids the "where did I put that repo" problem and keeps the filesystem clean. The random id prevents collisions when cloning repos with the same name from different orgs.

## Do not add `--quiet` to network git commands

`fetch_remote` inherits stderr (`Stdio::inherit()` + `.status()`) so git progress streams to the terminal during long operations. This means `git_err()` cannot extract error details from captured stderr — but git already printed the error in real time, so the sparse context string is sufficient. Local-only commands (`worktree add`, `branch -d`, etc.) continue to capture stderr with `.output()` so `git_err()` can format a clean single-line error. Best-effort network calls like `set_remote_head` keep captured stderr to avoid leaking confusing errors for intentionally silent failures.

On non-zero exit, `fetch_remote` returns only a short context error (`cannot fetch from '<remote>'`, which `clone` reports as `cannot clone <url>`) naming the step that failed — git has already printed the details above it. There is no silent-error sentinel: every `Err` reaching `main` is printed.

## Do not clone with `git clone --bare`

`git clone --bare` copies every remote branch into `refs/heads/` with no upstream, and the default fetch refspec then never updates them, so they silently go stale. `wt clone` instead runs `git init --bare`, `git remote add origin <url>` (standard refspec), and one `git fetch`. Only the default branch is created locally, tracking `origin/<default>`, and the bare repo's `HEAD` points at it. Every other branch is created on demand by `wt switch` as a tracking branch, exactly like a regular `git clone`.

## Do not track the start point of a new branch, unless it is the same-named remote branch

`wt switch feat origin/main` passes `--no-track`: a feature branch tracking `main` makes `git push` target (or be refused for) the base branch, and makes `prune` see an upstream that was never pushed to. When the start point is the remote copy of the same branch (`wt switch x upstream/x`, or `wt switch x` for a remote-only branch), the new branch tracks it. `switch` passes the full `refs/remotes/<remote>/<name>` ref so a same-named tag can never be checked out instead.

## Do not let `prune` delete the only copy of anything

`prune` runs unattended over every repo, so it only removes what is provably recoverable:

- Merged detection requires the branch to have been pushed (an upstream, or a same-named branch on a remote) unless `--base` or `--stale` opts in, because a fresh branch is trivially an ancestor of the base. Keep the `--base` opt-in: without a remote the base can't be auto-detected and nothing can be pushed, so `--base` is the only way to prune locally merged branches without `--stale`, which also deletes unmerged ones. The cost is that a fresh branch with no commits of its own is removed too
- The base branch, the remote's default branch, and the branch the repo's `HEAD` points at are never removed. `HEAD` covers bare repos with no remote, where no base can be resolved
- `--gone` removes a branch only if its changes reached the base: merged, or squash-merged (`git merge-tree --write-tree <base> <branch>` yields the base's own tree). A remote branch deleted unmerged, or new commits after a squash-merge, are skipped as `not merged`. There is no reliable record of what was pushed once the remote-tracking ref is pruned, so "content landed in base" is the only safe test
- Renamed worktree directories are repaired before `git worktree prune`, which would otherwise detach them and turn them into orphans
- Orphans (a `.git` file pointing at a missing repo) are only reported unless `--orphans` is passed: a moved repo or an unmounted volume looks identical to a deleted one, and the directory may hold uncommitted work
- Bare clones under `~/.wt/repos/` are removed once every worktree entry is prunable (a locked worktree may be on an unmounted volume, so it keeps its clone), and only if `git rev-list --all --not --remotes` is empty and its link store is empty: stashes, local tags, and shared files live in the clone, not the worktree. A clone with no remote-tracking refs is skipped: it may be a `wt clone` still fetching

Accepted gap: `git worktree remove` deletes gitignored files (build output, a hand-made `.env`). Treating them as dirty would block nearly every removal; files shared via `wt link` are symlinks in every worktree, so removal never touches the shared copy.

## Do not give `prune --dry-run` its own classification path

`prune --gone --dry-run` fetches like a real run instead of listing the remote's branches with `ls-remote`. A separate dry-run path can drift from what a real run removes. Fetching only updates remote-tracking refs; local branches and worktrees are untouched.

## Do not split the picker back into panes

The picker used to show a repos pane beside the selected repo's worktrees. That design had three problems:
- Enter and Esc meant different things depending on the focused pane and the filter.
- Narrow terminals switched to a repos-only mode where arrows could reach only each repo's first worktree.
- The open repo was unmarked once focus moved to the worktrees pane.

One list grouped by repo gives every key one meaning, and ←/→ jump between repos. In a prototype with 50 worktrees in 7 repos, reaching any worktree took 3.8 keys on average (6 at worst), versus 6.7 (11) with panes. The cost is seeing fewer repo names at once.

Within that design:
- ← goes to the top of the current repo before the previous repo (like vim's `{`), because a repo's main checkout is a common target.
- Clearing the filter keeps the highlighted worktree, so typing part of a repo name and pressing Esc lands inside that repo with the whole list around it.
- As the terminal narrows, columns give way in this order: lock badge, counts, repo names (down to 8 columns), branches (down to 16), then status.

## Do not mock git in tests

Tests run the real binary against real temp repos. Mocks hide the git version differences and filesystem edge cases that matter most for a tool that wraps git. Slower tests are acceptable at this codebase size.
