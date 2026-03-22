# wt

Git worktree manager.

`wt` creates worktrees under `~/.wt/worktrees/`, manages branch lifecycle, and keeps shared files in sync across worktrees.

## Features

- **Create and switch** — check out branches into isolated worktrees with `wt new`, or use `wt switch` to find an existing worktree or create one
- **Clean up** — `wt prune` removes worktrees whose branches are merged or whose upstream is gone
- **Link shared files** — `wt link .env` symlinks files from the primary worktree into all others, automatically applied to new worktrees
- **Typo detection** — `wt switch` catches misspelled branch names with fuzzy matching before creating a new branch
- **Script-friendly** — stdout is always data (paths, JSON); messages go to stderr

## Install

```sh
cargo install --path .
```

## Usage

```sh
wt new my-feature                    # check out existing branch
wt new -c my-feature                 # create new branch from HEAD
wt new -c my-feature develop         # create from base
wt new v2.0                          # check out tag (detached HEAD)
wt switch my-feature                 # get or create worktree
wt list                              # list worktrees
wt list --json                       # machine-readable JSON output
wt remove my-feature                 # remove worktree and branch
wt remove my-feature --keep-branch   # remove worktree, keep branch
wt path my-feature                   # print worktree path
wt prune                             # remove merged worktrees
wt prune --gone                      # also remove upstream-gone
wt prune --base develop              # override base branch
wt link .env .env.local              # symlink into all worktrees
wt link --list                       # show configured links
wt unlink .env                       # remove symlinks
wt unlink --all                      # remove all linked files
```

Short aliases: `n`, `s`, `ls`, `rm`, `p`, `ln`.

All commands accept `--repo <path>`. Run `wt <command> --help` for full options.

## Shell Integration

Add to your shell config for tab completion and auto-cd after `new` and `switch`:

```sh
# zsh (~/.zshrc)
eval "$(wt init zsh)"

# bash (~/.bashrc)
eval "$(wt init bash)"

# fish (~/.config/fish/config.fish)
wt init fish | source
```

If you previously used `wt completions` to generate a static completions file, remove it to avoid loading completions twice.
