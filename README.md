# wt

Git worktree manager. Work on multiple branches simultaneously — each in its own directory, sharing one repo.

```sh
wt                          # pick a worktree (interactive fuzzy picker)
wt clone <url>              # clone a repo and create the first worktree
wt switch my-feature        # cd into the branch's worktree, creating branch and worktree if needed
wt switch hotfix v1.0       # start a new branch from a branch, tag, or commit
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
| `wt switch <branch> [base]` | `s` | Find or create a worktree for a branch |
| `wt list [--all]` | `ls` | List worktrees (across all repos with `--all`) |
| `wt remove <branch>` | `rm` | Remove worktree, and its branch if merged |
| `wt prune` | | Remove worktrees whose pushed branches were merged or squash-merged |
| `wt link <file>` | `ln` | Share untracked files (e.g. `.env`) across worktrees |

## Highlights

- **Interactive picker** — run bare `wt` to browse all repos and worktrees with fuzzy search, status indicators, and keyboard navigation
- **Typo detection** — misspell a branch name and `wt` suggests the closest match before creating anything
- **Shared files** — `wt link .env` moves the file into the repo's git directory and symlinks it into every worktree, including new ones, so removing a worktree never loses it
- **Script-friendly** — stdout is always data (paths); messages go to stderr
