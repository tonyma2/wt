# Design Decisions

Intentional choices that look like they could be "improved." Read the rationale before changing any of these.

## Do not add error crates

No `anyhow`, `thiserror`, or custom error enums. Every error is a `String` that exits 1. Typed error recovery adds complexity with no benefit — the binary never catches and branches on error variants.

## Do not "fix" `worktree_status()`, `fast_forward()`, `tracks()`, or `is_ignored()` to match other `Git` methods

All `Git` methods run `git -C <admin_repo>`. `worktree_status()` (behind `Status::of()`, the only dirty check), `fast_forward()` (`pull`'s `merge --ff-only`), `tracks()` (the tracked-file check in `link`), and `is_ignored()` (`link`'s not-gitignored warning) run `git -C <worktree_path>` instead. This is correct: `git status`, `git merge`, `git ls-files`, and `git check-ignore` act on the tree and index they're pointed at, so running them against the admin repo silently gives the wrong answer (or fails, for a bare repo).

## Do not split `switch` into separate checkout and create commands

`wt new` (check out, `-c` to create) used to sit beside `wt switch` (get-or-create). Their errors pointed at each other, and the split existed only so typos couldn't create branches. `switch` guards against that with a Levenshtein check instead: a name close to an existing branch errors with a suggestion, and `-c` bypasses it when a similar name is intended. A `[base]` creates without the check, since naming a start point already states the intent. A name that exists only on a remote is checked out tracking it, never shadowed by a fresh branch from `HEAD`.

## Do not check out tags or SHAs as detached worktrees

`wt switch v1.0` errors and suggests `wt switch <branch> v1.0`. A name that only matches an abbreviated commit hash (a ticket number like `1234`) also suggests `-c`, which skips this check and creates a branch with that name. A detached worktree has no name to look it up by, so supporting them meant a second resolution path (ref → SHA → matching detached `HEAD`) in `rm` and zsh completion. Starting a branch from the ref covers the use case; `git worktree add --detach` still works for a one-off, and `wt rm <path>` removes it. Worktrees that become detached (mid-rebase, `git checkout --detach`) are still listed and removable by path.

## Do not print human-readable messages to stdout

Shell wrappers depend on `cd "$(wt switch ...)"` capturing stdout as a path. Any non-data output on stdout breaks downstream consumers. Status messages go to stderr.

## Do not replace string replacement with a custom clap completer

Zsh completions inject custom functions via string replacement on clap_complete's generated script. A custom clap completer would pull in more of clap's internals for the same result. Keep the complexity in shell, not Rust.

**Caveat:** the replacement strings (the `COMPLETERS` table in `init.rs`) are coupled to the `///` help text on `name`, `names`, `base`, and `files` args in `cli.rs`. A new positional arg needs a doc comment that differs from these, or a replacement target collides with it. Changing those docstrings silently degrades completion. `render()` errors when a target is missing, and the `zsh_wires_each_completer_into_its_subcommands` unit test catches collisions — run `cargo test init` after editing `cli.rs` arg help text.

## Do not add doc comments outside `cli.rs`

`///` doc comments in `cli.rs` serve a concrete purpose: clap derives `--help` text from them on structs that derive `Parser`/`Subcommand`. Everywhere else they have no mechanical effect — they'd just be prose attached to internal functions in a binary that has no public API consumers. Code is self-documenting; inline `//` comments explain non-obvious *why*, not *what*.

## Do not give shared files a source worktree

`wt link .env` moves the file into `<git-common-dir>/wt/links/` and symlinks it into every worktree, including the one it came from. Previously one worktree held the real file and the others linked to it; removing that worktree (`wt rm`, or `git worktree remove`, which deletes gitignored files) destroyed the only copy and left every link dangling. With the store, no worktree is special: `rm` and `prune` need no link checks, and every worktree resolves the same store via `--git-common-dir`.

- The store lives with the repo, not in `~/.wt/`: a per-repo store under `~/.wt/` needs a repo → id mapping (config again) and leaks when the repo is deleted. Inside the git directory it never shows up in the working tree, isn't copied by `git clone`, and goes away with the repo
- Only untracked files can be shared, never directories. With files only, the store's contents are the whole record of what is shared: there is no manifest to keep in step with it, and a run interrupted after the move is finished by running `wt link` again. A linked directory would need that record to tell itself apart from a directory that merely holds linked files
- Moving a file into the store never replaces an existing entry (hard link, or a create-only copy across filesystems): on a case-insensitive filesystem `.Env` is the shared `.env`, and two runs linking the same file at once would otherwise both move theirs in. This is why no lock is needed
- Symlinks are absolute: relative links break when only one side moves. Re-running `wt link` replaces links left dangling by a move
- There is no `unlink`: stopping sharing is rare enough to do by hand (replace each link with a copy, then delete the store file)

## Do not change cd hint TTY detection to check stderr

After creating a worktree, `switch` and `clone` print a `cd '<path>'` hint to **stderr** — but only when **stdout** is a TTY.

- In `cd "$(wt switch ...)"`, stdout is captured by the command substitution while stderr remains attached to the terminal
- Checking stdout-is-TTY (not stderr-is-TTY) correctly suppresses the hint for wrapper users (stdout piped) and shows it for bare invocations (stdout is the terminal)
- The hint uses the literal path rather than a lookup by branch name, which fails after `clone` (cwd is outside the repo). The path is single-quoted with `'` escaped as `'\''`
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

- "Merged" means the branch's changes are in the default branch: `git merge-tree --write-tree <base> <branch>` yields the base's own tree. That covers plain merges and squash merges, and fails for a branch with new commits after a squash merge or one deleted from the remote unmerged
- The branch must have been pushed (an upstream, or a same-named branch on a remote), because a fresh branch is trivially merged right after it is created. Tracking the base itself doesn't count. Repos without a remote are never pruned; `wt rm` handles them
- `origin` is fetched first so a merge made on the remote counts. If the fetch fails, local refs are used, which can only make prune miss a branch, never remove one it shouldn't
- The default branch and the branch the repo's `HEAD` points at are never removed. `HEAD` covers bare repos
- Orphans (a `.git` file pointing at a missing repo) are only reported: a moved repo or an unmounted volume looks identical to a deleted one, and the directory may hold uncommitted work. The report says to run `wt` inside a moved repo to reconnect it. A worktree that contains another worktree (e.g. Claude Code's `.claude/worktrees/`) is kept, since removing it deletes the inner one; `rm` refuses it even with `--force`
- `prune` doesn't run `git worktree prune`: it would drop the metadata of a worktree renamed by hand or on an unmounted volume, after which `git worktree repair` can't reconnect it. git's gc expires stale entries on its own, `switch` removes only the stale entry that blocks its branch, and `wt rm` removes one by branch or path

Accepted gap: `git worktree remove` deletes gitignored files (build output, a hand-made `.env`). Treating them as dirty would block nearly every removal; files shared via `wt link` are symlinks in every worktree, so removal never touches the shared copy.

## Do not replace targeted repair with a blanket `git worktree repair`

Renaming a main checkout breaks the `.git` file of every linked worktree, and nothing in `~/.wt/` records where the repo went (no config, by design). So `wt` reconnects a repo when it runs inside one: before every command except `init` and `clone`, it lists the current repo's worktrees and runs `git worktree repair <paths>` only for those whose `.git` file points at a location that no longer exists. Until then, the picker and `list --all` skip the missing repo and say how to reconnect it.

- Plain `git worktree repair` also rewrites links that point at a *different, valid* repo. Run in a `cp -R` copy of a checkout, it repoints the original's worktrees at the copy. A link is only touched when its target is gone
- The check costs one `git worktree list` per invocation; `init` and `clone` skip it because `init` runs on every shell startup and `clone` never runs inside the repo it acts on

## Do not let `pull` merge, rebase, or touch local changes

`pull` runs over every worktree of every repo at once, so it only does what needs no decision: fast-forward a clean worktree that is behind its upstream and not ahead of it. Everything else is skipped and reported, never attempted:

- Dirty worktrees (including untracked files, and a stopped merge or cherry-pick) that are behind are skipped, though `merge --ff-only` would refuse an overlapping change anyway: files changing under an open editor or a running dev server mid-work is the surprise to avoid. A dirty worktree with nothing to pull is just up to date; diverged and gone upstreams are reported as such, since local changes aren't what stops them
- Diverged branches are skipped rather than merged or rebased; a deleted upstream is reported as "upstream gone"
- Detached worktrees (including mid-rebase) and branches without an upstream are left out silently
- Each remote a checked-out branch tracks is fetched once per repo, not just `origin` and not `--all`: a branch tracking `upstream/main` updates, and a dead extra remote can't fail the run. If a fetch fails, the worktrees tracking that remote are skipped rather than fast-forwarded to stale refs, and the run exits 1
- Branches without a worktree are not updated: nothing reads them, since new branches start from `origin/<default>`

## Do not let the picker prompt for credentials

The picker holds the terminal in raw mode, so a credential or SSH passphrase prompt would hang or garble it, and parallel fetches would interleave their output. Its pull fetches with stdin closed, output captured, `GIT_TERMINAL_PROMPT=0`, and `SSH_ASKPASS_REQUIRE=force` with `SSH_ASKPASS=false`, which makes ssh decline passphrase and unknown-host-key prompts instead of reading `/dev/tty`. A repo that needs a prompt shows "fetch failed", and the summary points at `wt pull`, which streams git's output and can prompt. Quitting mid-pull doesn't wait: the captured git processes write nothing more and finish on their own.

## Do not drop the picker's remove confirmation

^x asks for a second ^x; any other key cancels and is swallowed. Removal uses `wt rm`'s checks without `--force`, so only clean, merged worktrees go, but `git worktree remove` still deletes gitignored files (`node_modules`, build output, a hand-made `.env`), and a stray key shouldn't cost those. There is no force from the picker; `wt rm --force` is the deliberate path. Remove is refused while a pull is running, since a pull may be fast-forwarding that worktree.

## Do not add flags to `prune`

`prune` has one rule and `--dry-run`. `--gone`, `--stale`, `--base`, `--orphans`, and `--repo` were removed, along with sweeps for empty directories and unused bare clones: each covered a case `wt rm` (or `rm -rf`) handles, and together they multiplied the classification paths and their tests. A branch `prune` won't remove is removed with `wt rm`.

## Do not give `prune --dry-run` its own classification path

`prune --dry-run` fetches like a real run instead of listing the remote's branches with `ls-remote`. A separate dry-run path can drift from what a real run removes. Fetching only updates remote-tracking refs; local branches and worktrees are untouched.

## Do not add a `--repo` flag

Commands act on the repository containing the current directory; `cd` into it or use the picker. `--repo` was on seven commands, used about once, and every shell completer had to parse it to know which repo to query.

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
