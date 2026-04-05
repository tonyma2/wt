# wt

Git worktree manager. Work on multiple branches simultaneously — each in its own directory, sharing one repo.

```sh
wt                          # pick a worktree (interactive fuzzy picker)
wt clone <url>              # clone a repo and create the first worktree
wt new -c my-feature        # create a branch and cd into its worktree
wt switch my-feature        # jump to an existing worktree, or create one
wt prune                    # clean up worktrees for merged branches
```

## Install

```sh
cargo install --path .
```

### Shell integration

Add to your shell config for tab completion and auto-cd:

```sh
# zsh — after compinit
eval "$(wt init zsh)"

# bash
eval "$(wt init bash)"

# fish
wt init fish | source
```

## Commands

| Command | Alias | What it does |
|---------|-------|--------------|
| `wt` | | Interactive picker with fuzzy filtering |
| `wt clone <url>` | `cl` | Clone repo, create first worktree |
| `wt new <branch>` | `n` | Check out a branch or ref into a new worktree |
| `wt switch <branch>` | `s` | Find or create a worktree for a branch |
| `wt list [--json]` | `ls` | List worktrees (JSON for scripts) |
| `wt remove <branch>` | `rm` | Remove worktree and delete branch |
| `wt path <branch>` | `p` | Print worktree path |
| `wt prune [--gone]` | | Remove merged (and upstream-gone) worktrees |
| `wt link <file>` | `ln` | Symlink shared files across worktrees |
| `wt unlink <file>` | | Remove symlinked files |

## Highlights

- **Interactive picker** — run bare `wt` to browse all repos and worktrees with fuzzy search, status indicators, and keyboard navigation
- **Typo detection** — misspell a branch name and `wt` suggests the closest match before creating anything
- **Shared files** — `wt link .env` symlinks files from the primary worktree into all others, automatically applied to new worktrees
- **Script-friendly** — stdout is always data (paths, JSON); messages go to stderr
