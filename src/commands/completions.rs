use clap::CommandFactory;

use crate::cli::Cli;

pub fn run(shell: clap_complete::Shell) -> Result<(), String> {
    let script = render(shell);
    print!("{script}");
    Ok(())
}

fn render(shell: clap_complete::Shell) -> String {
    let mut out = Vec::new();
    clap_complete::generate(shell, &mut Cli::command(), "wt", &mut out);
    let mut script = String::from_utf8_lossy(&out).into_owned();

    if shell == clap_complete::Shell::Zsh {
        let helper = r#"

_wt_collect_worktree_rows() {
    local -a cmd flags
    local i line wt_path branch repo_arg
    typeset -ga _wt_completion_branches _wt_completion_paths _wt_completion_flags
    _wt_completion_branches=()
    _wt_completion_paths=()
    _wt_completion_flags=()
    cmd=(command wt list --porcelain)
    for (( i = 1; i <= ${#words[@]}; i++ )); do
        if [[ ${words[i]} == --repo=* ]]; then
            repo_arg="${words[i]#--repo=}"
            if [[ $repo_arg == "~" ]]; then
                repo_arg="$HOME"
            elif [[ $repo_arg == "~/"* ]]; then
                repo_arg="$HOME/${repo_arg#~/}"
            fi
            cmd+=(--repo "$repo_arg")
            break
        fi
        if [[ ${words[i]} == "--repo" && -n ${words[i+1]:-} ]]; then
            repo_arg="${words[i+1]}"
            if [[ $repo_arg == "~" ]]; then
                repo_arg="$HOME"
            elif [[ $repo_arg == "~/"* ]]; then
                repo_arg="$HOME/${repo_arg#~/}"
            fi
            cmd+=(--repo "$repo_arg")
            break
        fi
    done
    while IFS= read -r line; do
        if [[ $line == worktree\ * ]]; then
            if [[ -n ${wt_path:-} ]]; then
                _wt_completion_branches+=("$branch")
                _wt_completion_paths+=("$wt_path")
                _wt_completion_flags+=("${flags[*]}")
            fi
            wt_path=${line#worktree }
            branch=""
            flags=()
        elif [[ $line == branch\ refs/heads/* ]]; then
            branch=${line#branch refs/heads/}
        elif [[ $line == detached ]]; then
            flags+=(detached)
        elif [[ $line == locked* ]]; then
            flags+=(locked)
        elif [[ $line == prunable* ]]; then
            flags+=(prunable)
        elif [[ -z $line ]]; then
            if [[ -n ${wt_path:-} ]]; then
                _wt_completion_branches+=("$branch")
                _wt_completion_paths+=("$wt_path")
                _wt_completion_flags+=("${flags[*]}")
            fi
            wt_path=""
            branch=""
            flags=()
        fi
    done < <("${cmd[@]}" 2>/dev/null)
    if [[ -n ${wt_path:-} ]]; then
        _wt_completion_branches+=("$branch")
        _wt_completion_paths+=("$wt_path")
        _wt_completion_flags+=("${flags[*]}")
    fi
    (( ${#_wt_completion_paths[@]} > 0 ))
}

_wt_complete_branches_with_paths() {
    local -a values descs
    local idx max_branch=0 details path_display
    local cols=${COLUMNS:-0}
    local max_path=72

    _wt_collect_worktree_rows || return 1

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        [[ -z ${_wt_completion_branches[idx]} ]] && continue
        values+=("${_wt_completion_branches[idx]}")
        (( ${#_wt_completion_branches[idx]} > max_branch )) && max_branch=${#_wt_completion_branches[idx]}
    done
    if (( ${#values[@]} == 0 )); then
        return 1
    fi
    if (( cols > max_branch + 12 )); then
        max_path=$(( cols - max_branch - 8 ))
    fi
    (( max_path < 24 )) && max_path=24

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        [[ -z ${_wt_completion_branches[idx]} ]] && continue
        path_display="${_wt_completion_paths[idx]}"
        if (( ${#path_display} > max_path )); then
            path_display="...${path_display[-$((max_path - 3)),-1]}"
        fi
        details="$path_display"
        if [[ $idx -eq 1 ]]; then
            details="$details [main]"
        fi
        if [[ -n ${_wt_completion_flags[idx]} ]]; then
            details="$details [${_wt_completion_flags[idx]}]"
        fi
        descs+=("$(printf "%-${max_branch}s  %s" "${_wt_completion_branches[idx]}" "$details")")
    done
    compadd -l -d descs -- "${values[@]}"
}

_wt_path_branches() {
    _wt_complete_branches_with_paths
}

_wt_remove_targets() {
    _wt_complete_branches_with_paths
}
"#;
        let dispatch_marker = "if [ \"$funcstack[1]\" = \"_wt\" ]; then";
        if let Some(idx) = script.find(dispatch_marker) {
            script.insert_str(idx, helper);
        } else {
            script.push_str(helper);
        }
        script = script.replace(
            ":name -- Worktree branch name:_default",
            ":name -- Worktree branch name:_wt_path_branches",
        );
        script = script.replace(
            "*::names -- Branch names or paths:_default",
            "*::names -- Branch names or paths:_wt_remove_targets",
        );
    }

    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_path_completion_is_dynamic() {
        let script = render(clap_complete::Shell::Zsh);
        assert!(script.contains("_wt_path_branches()"));
        assert!(script.contains("_wt_remove_targets()"));
        assert!(script.contains("_wt_collect_worktree_rows()"));
        assert!(script.contains("_wt_complete_branches_with_paths()"));
        assert!(
            script.find("_wt_path_branches()").unwrap()
                < script
                    .find("if [ \"$funcstack[1]\" = \"_wt\" ]; then")
                    .unwrap()
        );
        assert_eq!(
            script
                .matches(":name -- Worktree branch name:_wt_path_branches")
                .count(),
            2
        );
        assert_eq!(
            script
                .matches("*::names -- Branch names or paths:_wt_remove_targets")
                .count(),
            2
        );
        assert!(!script.contains("[path target]"));
    }

    #[test]
    fn bash_completion_does_not_include_zsh_helper() {
        let script = render(clap_complete::Shell::Bash);
        assert!(!script.contains("_wt_path_branches()"));
    }
}
