# wt

Git worktree manager. Creates worktrees under `~/.worktrees/<repo>/` and manages their lifecycle.

## Install

```sh
cargo install --path .
```

Add a shell wrapper to get `cd` behavior after `wt new` and `wt path`:

```sh
wt() {
  case "$1" in
    new|n|path|p)
      local out
      out=$(command wt "$@") || return
      if [[ -n "$out" && -d "$out" ]]; then
        cd "$out"
      else
        printf '%s\n' "$out"
      fi
      ;;
    *) command wt "$@" ;;
  esac
}
```

Install shell completion support:

```sh
wt init-shell
# or explicitly:
wt init-shell --shell zsh
```

## Usage

```
wt new <branch>          Create a worktree (alias: n)
wt remove <branch>...    Remove worktrees and their branches (alias: rm)
wt link <file>...        Link files from primary worktree (alias: ln)
wt list                  List worktrees (alias: ls)
wt path <branch>         Print worktree path (alias: p)
wt prune                 Clean up stale and orphaned worktrees
wt init-shell            Install shell completions (bash/zsh/fish)
```

### Options

```
--repo <path>    Specify repository (default: current repo)
--force          Force removal of dirty worktrees
--dry-run, -n    Show what prune would do without doing it
```
