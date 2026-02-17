# Git Workflow

Agents own the ceremony — commits, PRs. The human owns the merge decision.

## Branches

Branch name: `<type>/<kebab-description>` (e.g. `feat/prune-locked`, `fix/path-resolve`).

## Commits

Conventional commits — imperative mood, lowercase, no period. No additional description in the commit body unless the subject line alone doesn't capture the change.

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

One logical change per commit.

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

**Creation** — push with `git push -u origin HEAD`, then `gh pr create`. Post the PR URL back to the human and end your turn.

## PR Updates

Commit and push. If the change type or scope shifted, update the title with `gh pr edit --title`. If the summary shifted, update the body with `gh pr edit --body`.

Rebase when behind `main` — always `--force-with-lease`, never `--force`:

```sh
git pull --rebase origin main
git push --force-with-lease
```

## Review

Do not merge or close the PR. The human will review and either approve or request changes. If changes are requested, address them per [PR Updates](#pr-updates) and end your turn. Only merge when the human explicitly asks.

## Merging

Squash merge is the default. `gh pr merge --squash` uses the PR title as the commit subject and appends `(#N)` automatically.

Always pass `--body`: collect `Co-Authored-By` trailers and `BREAKING CHANGE:` footers from the PR's commits, or use `--body ""` if there are none.

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
