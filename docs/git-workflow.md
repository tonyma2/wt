# Git Workflow

Agents own the ceremony — commits, PRs, CI. Humans own the merge decision.

## Commits

Conventional commits — imperative mood, lowercase, no period.

```
<type>[(<scope>)][!]: <description>
```

| Field     | Detail                                                                                     |
|-----------|--------------------------------------------------------------------------------------------|
| **Type**  | `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`                                  |
| **Scope** | Optional — subcommand or module name (`prune`, `new`, `list`, `rm`, `link`, `path`)        |
| **`!`**   | Before `:` to signal a breaking change                                                     |
| **Footer**| `BREAKING CHANGE: explanation` when impact isn't obvious; git trailers (`Co-Authored-By`)  |

Examples:

```
feat(prune): prune merged and upstream-gone worktrees
feat!: explicit -c/--create flag for wt new, remove auto-fetch
fix: warn on link conflicts instead of silently skipping
docs: use homebrew zsh site-functions path for completions
```

## Pull Requests

Title is the conventional commit subject — no `(#N)`, that's added at merge. Keep the title accurate as scope evolves.

```sh
gh pr create --title "<type>[(<scope>)][!]: <description>" --body "$(cat <<'EOF'
## Summary
- <1-3 bullets: what changed and why>

## Test plan
- `test_name` — what it verifies
- All N tests pass
EOF
)"
```

## CI

All four checks must pass before merge:

| Job        | Command                                                            |
|------------|--------------------------------------------------------------------|
| **fmt**    | `cargo fmt --check`                                                |
| **clippy** | `cargo clippy --locked --all-targets --all-features -- -D warnings`|
| **test**   | `cargo test --locked`                                              |
| **build**  | `cargo build --locked`                                             |

Fix failures locally, commit as separate commits, push. The squash merge folds them together.

## PR Updates

Commit and push. Update the PR title if the change type or scope shifted, and the PR body if the test plan changed — both via `gh pr edit`.

Rebase when behind `main` — always `--force-with-lease`, never `--force`:

```sh
git fetch origin main
git rebase origin/main
git push --force-with-lease
```

## Review

After CI passes, stop and wait. Do not self-merge. A human reviews and either approves or requests changes. If changes are requested, address them per [PR Updates](#pr-updates) and wait for another review cycle. The agent merges only after explicit human approval.

## Merging

Squash merge is the default. `gh pr merge --squash` uses the PR title as the commit subject and appends `(#N)` automatically.

Always pass `--body` to prevent GitHub from concatenating all commit messages into the body. Use collected git trailers and `BREAKING CHANGE:` footers, or `--body ""` for a clean commit.

Before merging, validate the PR title against all commits on the branch:

- Promote the type if a higher-impact change was added (`feat` > `fix` > `docs` > others)
- Add `!` if any commit introduced a breaking change
- Override with `--subject` only if the title is stale

Use `gh pr merge --rebase` only when every commit is a well-formed conventional commit worth preserving individually.

## Cleanup

| Scenario        | Command                                |
|-----------------|----------------------------------------|
| Abandoned PR    | `gh pr close <N> --delete-branch`      |
| Stale branch    | `git push origin --delete <branch>`    |
