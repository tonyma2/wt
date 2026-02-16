# Git Workflow

Agents own the ceremony — commits, PRs, CI. Humans own the merge decision.

## Branches

Branch name: `<type>/<kebab-description>` (e.g. `feat/prune-locked`, `fix/path-resolve`).

## Commits

Conventional commits — imperative mood, lowercase, no period.

```
<type>[(<scope>)][!]: <description>
```

| Field     | Detail                                                                                    |
|-----------|-------------------------------------------------------------------------------------------|
| **Type**  | `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`                                  |
| **Scope** | Optional — subcommand or module name (`prune`, `new`, `list`, `rm`, `link`, `path`)       |
| **`!`**   | Before `:` to signal a breaking change                                                    |
| **Footer**| `BREAKING CHANGE: explanation` when impact isn't obvious; git trailers (`Co-Authored-By`) |

Examples:

```
feat(prune): prune merged and upstream-gone worktrees
feat!: explicit -c/--create flag for wt new, remove auto-fetch
fix: warn on link conflicts instead of silently skipping
docs: use homebrew zsh site-functions path for completions
```

## Pull Requests

**Title** — the conventional commit subject. No `(#N)` suffix; that's added at merge. Keep the title accurate as scope evolves.

**Body** — a `## Summary` heading with 1-2 sentences of motivation followed by a before/after table:

```markdown
## Summary

<motivation — why this change exists>

| Before | After |
|--------|-------|
| old behavior or "N/A" for new features | new behavior |
```

- One row per user-visible behavioral change — skip internal refactors
- Describe behaviors, not code (`prune ignores locked worktrees` not `added if locked { continue }`)

## CI

All four checks must pass before merge:

| Job        | Command                                                              |
|------------|----------------------------------------------------------------------|
| **fmt**    | `cargo fmt --check`                                                  |
| **clippy** | `cargo clippy --locked --all-targets --all-features -- -D warnings`  |
| **test**   | `cargo test --locked`                                                |
| **build**  | `cargo build --locked`                                               |

Fix failures locally, commit as separate commits, push. The squash merge folds them together.

## PR Updates

Commit and push. If the change type or scope shifted, update the title with `gh pr edit --title`. If the summary shifted, update the body with `gh pr edit --body`.

Rebase when behind `main` — always `--force-with-lease`, never `--force`:

```sh
git fetch origin main
git rebase origin/main
git push --force-with-lease
```

## Review

After CI passes, do not merge, close, or take further action on the PR. A human will review and either approve or request changes. If changes are requested, address them per [PR Updates](#pr-updates) and wait for the next review cycle. Only merge when the human explicitly asks.

## Merging

Squash merge is the default. `gh pr merge --squash` uses the PR title as the commit subject and appends `(#N)` automatically.

Always pass `--body` to prevent GitHub from concatenating all commit messages into the body. Use collected git trailers and `BREAKING CHANGE:` footers, or `--body ""` for a clean commit.

Before merging, check the PR title against all commits in the PR:

- Promote the type if a higher-impact change was added (`feat` > `fix` > `docs` > others)
- Add `!` if any commit introduced a breaking change
- Update with `gh pr edit --title` if the title is stale

Use `gh pr merge --rebase` only when every commit is a well-formed conventional commit worth preserving individually.

## Cleanup

| Scenario        | Command                                |
|-----------------|----------------------------------------|
| Abandoned PR    | `gh pr close <N> --delete-branch`      |
| Stale branch    | `git push origin --delete <branch>`    |
