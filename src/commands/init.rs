use clap::CommandFactory;

use crate::cli::Cli;

const SH_WRAPPER: &str = "wt() {
  if [ $# -eq 0 ]; then
    local __wt_cd
    __wt_cd=$(mktemp) || return 1
    __WT_CD=\"$__wt_cd\" command wt
    local ret=$?
    if [ -s \"$__wt_cd\" ]; then cd -- \"$(cat \"$__wt_cd\")\" || ret=$?; fi
    rm -f \"$__wt_cd\"
    return $ret
  fi
  case \"$1\" in
    switch|s|clone|cl)
      local out
      out=$(command wt \"$@\") || return
      if [ -d \"$out\" ]; then cd -- \"$out\"; elif [ -n \"$out\" ]; then printf '%s\\n' \"$out\"; fi ;;
    *) command wt \"$@\" ;;
  esac
}
";

const FISH_WRAPPER: &str = "function wt --wraps=wt
  if test (count $argv) -eq 0
    set -l __wt_cd (mktemp); or return 1
    __WT_CD=$__wt_cd command wt
    set -l ret $status
    if test -s $__wt_cd
      cd -- (cat $__wt_cd); or set ret $status
    end
    rm -f $__wt_cd
    return $ret
  end
  switch $argv[1]
    case switch s clone cl
      set -l out (command wt $argv)
      or return
      if test -d \"$out\"
        cd -- $out
      else if test -n \"$out\"
        printf '%s\\n' $out
      end
    case '*'
      command wt $argv
  end
end
";

const COMPLETERS: [(&str, &str); 7] = [
    (
        ":name -- Branch checked out in the worktree:",
        "_wt_path_branches",
    ),
    (":name -- Branch name:", "_wt_switch_targets"),
    (
        "::base -- Start point for a new branch:",
        "_wt_local_branch_names",
    ),
    (
        "*::names -- Branch names or worktree paths:",
        "_wt_remove_targets",
    ),
    (
        "--base=[Base branch for merged detection (e.g. develop, trunk)]:BASE:",
        "_wt_local_branch_names",
    ),
    (
        "*::files -- Files or directories to link:",
        "_wt_link_files",
    ),
    (
        "*::files -- Files or directories to unlink:",
        "_wt_unlink_files",
    ),
];

const DISPATCH_MARKER: &str = "if [ \"$funcstack[1]\" = \"_wt\" ]; then";

pub fn run(shell: clap_complete::Shell) -> Result<(), String> {
    let script = render(shell)?;
    print!("{script}");
    Ok(())
}

fn render(shell: clap_complete::Shell) -> Result<String, String> {
    let mut out = Vec::new();
    clap_complete::generate(shell, &mut Cli::command(), "wt", &mut out);
    let mut script = String::from_utf8_lossy(&out).into_owned();

    if shell == clap_complete::Shell::Zsh {
        if let Some(rest) = script.strip_prefix("#compdef wt\n") {
            script = rest.to_owned();
        }

        let helper = r#"

_wt_extract_repo_args() {
    local i repo_arg
    typeset -ga _wt_repo_args
    typeset -g _wt_repo_path
    _wt_repo_args=()
    _wt_repo_path=""
    for (( i = 1; i <= ${#words[@]}; i++ )); do
        if [[ ${words[i]} == --repo=* ]]; then
            repo_arg=${words[i]#--repo=}
        elif [[ ${words[i]} == --repo && -n ${words[i+1]:-} ]]; then
            repo_arg=${words[i+1]}
        else
            continue
        fi
        [[ $repo_arg == "~" || $repo_arg == "~/"* ]] && repo_arg="$HOME${repo_arg:1}"
        _wt_repo_args=(--repo "$repo_arg")
        _wt_repo_path=$repo_arg
        return
    done
}

_wt_git() {
    _wt_extract_repo_args
    if [[ -n $_wt_repo_path ]]; then
        git -C "$_wt_repo_path" "$@" 2>/dev/null
    else
        git "$@" 2>/dev/null
    fi
}

_wt_collect_worktree_rows() {
    local line wt_path="" branch=""
    typeset -ga _wt_completion_branches _wt_completion_paths
    _wt_completion_branches=() _wt_completion_paths=()
    # the trailing echo guarantees a blank line after the last record
    while IFS= read -r line; do
        case $line in
            worktree\ *) wt_path=${line#worktree } ;;
            branch\ refs/heads/*) branch=${line#branch refs/heads/} ;;
            "")
                if [[ -n $wt_path ]]; then
                    _wt_completion_branches+=("$branch")
                    _wt_completion_paths+=("$wt_path")
                fi
                wt_path="" branch="" ;;
        esac
    done < <(_wt_git worktree list --porcelain; echo)
    (( ${#_wt_completion_paths[@]} > 0 ))
}

_wt_collect_local_branches() {
    typeset -ga _wt_local_branches
    _wt_local_branches=(${(f)"$(_wt_git for-each-ref --format='%(refname:short)' refs/heads/)"})
}

# remote branches by short name ("origin/feat/x" -> "feat/x"), deduplicated across remotes
_wt_collect_remote_branches() {
    typeset -ga _wt_remote_branches
    _wt_remote_branches=(${(f)"$(_wt_git for-each-ref --format='%(refname:lstrip=3)' refs/remotes/)"})
    _wt_remote_branches=(${(u)_wt_remote_branches:#HEAD})
}

# operands already typed, minus the subcommand names passed as arguments, flags, and --repo's value
_wt_collect_typed() {
    local -a skip=(wt "$@")
    local i w
    typeset -gA _wt_seen
    _wt_seen=()
    for (( i = 1; i <= ${#words[@]}; i++ )); do
        (( i == CURRENT )) && continue
        w=${words[i]}
        [[ -z $w ]] && continue
        (( ${skip[(Ie)$w]} )) && continue
        if [[ $w == -* ]]; then
            [[ $w == --repo ]] && (( i++ ))
            continue
        fi
        _wt_seen[$w]=1
    done
}

_wt_path_branches() {
    _wt_collect_worktree_rows || return 1
    local -a branches=(${_wt_completion_branches:#})
    (( ${#branches} > 0 )) && compadd -- "${branches[@]}"
}

# starts at 2: git lists the main worktree (or bare repo) first, and it can't be removed
_wt_remove_targets() {
    local -a values
    local idx b
    _wt_collect_worktree_rows || return 1
    _wt_collect_typed rm remove
    for (( idx = 2; idx <= ${#_wt_completion_paths[@]}; idx++ )); do
        b=${_wt_completion_branches[idx]}
        [[ -n $b ]] || continue
        (( ${+_wt_seen[$b]} || ${+_wt_seen[${_wt_completion_paths[idx]}]} )) && continue
        values+=("$b")
    done
    (( ${#values} > 0 )) && compadd -- "${values[@]}"
}

_wt_switch_targets() {
    local ret=1
    _wt_collect_worktree_rows
    _wt_collect_local_branches
    _wt_collect_remote_branches
    local -a worktrees=(${_wt_completion_branches:#})
    local -a branches=(${_wt_local_branches} ${_wt_remote_branches})
    branches=(${(u)branches:|worktrees})
    (( ${#worktrees} > 0 )) && compadd -V worktrees -X '%Bworktrees%b' -- "${worktrees[@]}" && ret=0
    (( ${#branches} > 0 )) && compadd -V branches -X '%Bbranches%b' -- "${branches[@]}" && ret=0
    return ret
}

_wt_local_branch_names() {
    _wt_collect_local_branches
    (( ${#_wt_local_branches[@]} > 0 )) && compadd -- "${_wt_local_branches[@]}"
}

_wt_link_files() {
    local root
    root=$(_wt_git rev-parse --show-toplevel) || return 1
    _path_files -W "$root"
}

_wt_unlink_files() {
    local -a results
    local item
    _wt_collect_typed unlink
    _wt_extract_repo_args
    for item in ${(f)"$(command wt link --list "${_wt_repo_args[@]}" 2>/dev/null)"}; do
        (( ${+_wt_seen[$item]} )) || results+=("$item")
    done
    (( ${#results[@]} > 0 )) && compadd -- "${results[@]}"
}
"#;
        if let Some(idx) = script.find(DISPATCH_MARKER) {
            script.insert_str(idx, helper);
        } else {
            script.push_str(helper);
        }
        for (spec, completer) in COMPLETERS {
            let target = format!("{spec}_default");
            if !script.contains(&target) {
                return Err(format!(
                    "cannot generate zsh completions: clap_complete output format changed \
                     ({completer} target not found), please report this bug"
                ));
            }
            script = script.replace(&target, &format!("{spec}{completer}"));
        }
    }

    match shell {
        clap_complete::Shell::Zsh | clap_complete::Shell::Bash => {
            script.push_str(SH_WRAPPER);
        }
        clap_complete::Shell::Fish => script.push_str(FISH_WRAPPER),
        _ => {}
    }

    Ok(script)
}

#[cfg(test)]
mod tests {
    use clap_complete::Shell;

    use super::*;

    #[test]
    fn zsh_wires_each_completer_into_its_subcommands() {
        let script = render(Shell::Zsh).unwrap();
        assert!(!script.starts_with("#compdef"));
        // One use per subcommand name and alias; extra uses mean another arg's help text
        // collides with a replacement target.
        for (completer, uses) in [
            ("_wt_path_branches", 2),
            ("_wt_switch_targets", 2),
            ("_wt_local_branch_names", 3),
            ("_wt_remove_targets", 2),
            ("_wt_link_files", 2),
            ("_wt_unlink_files", 1),
        ] {
            let used = script.matches(&format!(":{completer}'")).count();
            assert_eq!(used, uses, "{completer}");
        }
        let dispatch = script.find(DISPATCH_MARKER).unwrap();
        assert!(script.find("_wt_path_branches() {").unwrap() < dispatch);
    }

    #[test]
    fn appends_wrapper_for_each_shell() {
        assert!(render(Shell::Zsh).unwrap().ends_with(SH_WRAPPER));
        assert!(render(Shell::Bash).unwrap().ends_with(SH_WRAPPER));
        assert!(render(Shell::Fish).unwrap().ends_with(FISH_WRAPPER));
    }

    #[test]
    fn completion_helpers_are_zsh_only() {
        for shell in [Shell::Bash, Shell::Fish] {
            assert!(!render(shell).unwrap().contains("_wt_collect_worktree_rows"));
        }
    }
}
