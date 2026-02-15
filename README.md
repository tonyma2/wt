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

Enable zsh tab completion:

```sh
wt completions zsh > "$(brew --prefix)/share/zsh/site-functions/_wt"
```

## Usage

```
wt new <name> [--repo <path>]                      Check out an existing branch or ref (alias: n)
wt new -c <name> [base] [--repo <path>]            Create a new branch (optionally from [base]) (alias: n)
wt remove <branch>... [--force] [--repo <path>]    Remove worktrees and their branches (alias: rm)
wt link <file>... [--force] [--repo <path>]        Link files from primary worktree (alias: ln)
wt list [--repo <path>]                            List worktrees (alias: ls)
wt path <branch> [--repo <path>]                   Print worktree path (alias: p)
wt prune [--dry-run] [--repo <path>]               Clean up stale and orphaned worktrees
wt completions <shell>                             Generate shell completions (bash/zsh/fish)
```

### Options

```
--repo <path>    Specify repository (default: current repo)
--force          Force operation (rm: remove dirty worktrees; link: replace conflicting targets)
--dry-run, -n    Show what prune would do without doing it
-c, --create     Create a new branch (wt new only; [base] requires --create)
```
