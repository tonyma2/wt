use clap::CommandFactory;

use crate::cli::Cli;

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
        let helper = r#"

_wt_extract_repo_args() {
    local i repo_arg
    typeset -ga _wt_repo_args
    typeset -g _wt_repo_path
    _wt_repo_args=()
    _wt_repo_path=""
    for (( i = 1; i <= ${#words[@]}; i++ )); do
        if [[ ${words[i]} == --repo=* ]]; then
            repo_arg="${words[i]#--repo=}"
            if [[ $repo_arg == "~" ]]; then
                repo_arg="$HOME"
            elif [[ $repo_arg == "~/"* ]]; then
                repo_arg="$HOME/${repo_arg#~/}"
            fi
            _wt_repo_args=(--repo "$repo_arg")
            _wt_repo_path="$repo_arg"
            return
        fi
        if [[ ${words[i]} == "--repo" && -n ${words[i+1]:-} ]]; then
            repo_arg="${words[i+1]}"
            if [[ $repo_arg == "~" ]]; then
                repo_arg="$HOME"
            elif [[ $repo_arg == "~/"* ]]; then
                repo_arg="$HOME/${repo_arg#~/}"
            fi
            _wt_repo_args=(--repo "$repo_arg")
            _wt_repo_path="$repo_arg"
            return
        fi
    done
}

_wt_collect_worktree_rows() {
    local -a cmd flags
    local line wt_path branch head
    typeset -ga _wt_completion_branches _wt_completion_paths _wt_completion_flags _wt_completion_heads
    typeset -g _wt_main_path
    _wt_completion_branches=()
    _wt_completion_paths=()
    _wt_completion_flags=()
    _wt_completion_heads=()
    _wt_main_path=""
    _wt_extract_repo_args
    cmd=(command wt list --porcelain "${_wt_repo_args[@]}")
    while IFS= read -r line; do
        if [[ $line == worktree\ * ]]; then
            if [[ -n ${wt_path:-} ]]; then
                _wt_completion_branches+=("$branch")
                _wt_completion_paths+=("$wt_path")
                _wt_completion_flags+=("${flags[*]}")
                _wt_completion_heads+=("$head")
            fi
            wt_path=${line#worktree }
            [[ -z $_wt_main_path ]] && _wt_main_path="$wt_path"
            branch=""
            head=""
            flags=()
        elif [[ $line == branch\ refs/heads/* ]]; then
            branch=${line#branch refs/heads/}
        elif [[ $line == HEAD\ * ]]; then
            head=${line#HEAD }
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
                _wt_completion_heads+=("$head")
            fi
            wt_path=""
            branch=""
            head=""
            flags=()
        fi
    done < <("${cmd[@]}" 2>/dev/null)
    if [[ -n ${wt_path:-} ]]; then
        _wt_completion_branches+=("$branch")
        _wt_completion_paths+=("$wt_path")
        _wt_completion_flags+=("${flags[*]}")
        _wt_completion_heads+=("$head")
    fi
    (( ${#_wt_completion_paths[@]} > 0 ))
}

_wt_collect_local_branches() {
    local -a cmd
    typeset -ga _wt_local_branches
    _wt_local_branches=()
    _wt_extract_repo_args
    cmd=(git)
    [[ -n $_wt_repo_path ]] && cmd+=(-C "$_wt_repo_path")
    cmd+=(for-each-ref --format='%(refname:short)' refs/heads/)
    _wt_local_branches=("${(@f)$(${cmd[@]} 2>/dev/null)}")
}

_wt_collect_tags() {
    local -a cmd
    typeset -ga _wt_tags _wt_tag_shas
    _wt_tags=()
    _wt_tag_shas=()
    _wt_extract_repo_args
    cmd=(git)
    [[ -n $_wt_repo_path ]] && cmd+=(-C "$_wt_repo_path")
    local tag sha
    while IFS=' ' read -r tag sha; do
        _wt_tags+=("$tag")
        _wt_tag_shas+=("$sha")
    done < <("${cmd[@]}" for-each-ref \
        --format='%(refname:short) %(if)%(*objectname)%(then)%(*objectname)%(else)%(objectname)%(end)' \
        refs/tags/ 2>/dev/null)
}

_wt_setup_colors() {
    typeset -g worktree_color=$'\e[36m' current_color=$'\e[32m' bold_yellow=$'\e[1;33m'
    typeset -g prunable_color=$'\e[31m' dim=$'\e[2m' dim_yellow=$'\e[2;33m' reset=$'\e[0m'
    if [[ -n ${NO_COLOR+x} || ${TERM:-} == dumb ]]; then
        typeset -g worktree_color="" current_color="" bold_yellow="" prunable_color="" dim="" dim_yellow="" reset=""
    fi
}

_wt_find_current_branch() {
    local physical_pwd=${PWD:A}
    local idx p best_len=0 best_idx=0
    typeset -g _wt_current_branch=""
    for (( idx = 1; idx <= ${#_wt_completion_paths[@]}; idx++ )); do
        p="${_wt_completion_paths[idx]}"
        if [[ $physical_pwd == $p || $physical_pwd == $p/* ]]; then
            if (( ${#p} > best_len )); then
                best_len=${#p}
                best_idx=$idx
            fi
        fi
    done
    (( best_idx > 0 )) && _wt_current_branch="${_wt_completion_branches[best_idx]}"
}

_wt_complete_branches_with_paths() {
    local -a values descs
    local idx max_branch=0 details path_display branch_color current_branch="" b flag
    local cols=${COLUMNS:-0}
    local max_path=72
    _wt_setup_colors

    _wt_collect_worktree_rows || return 1

    _wt_find_current_branch
    current_branch=$_wt_current_branch

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        b="${_wt_completion_branches[idx]}"
        [[ -z $b ]] && continue
        values+=("$b")
        (( ${#b} > max_branch )) && max_branch=${#b}
    done
    if (( ${#values[@]} == 0 )); then
        return 1
    fi
    if (( cols > max_branch + 14 )); then
        max_path=$(( cols - max_branch - 10 ))
    fi
    (( max_path < 24 )) && max_path=24

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        b="${_wt_completion_branches[idx]}"
        [[ -z $b ]] && continue
        path_display="${_wt_completion_paths[idx]/#${HOME}/~}"
        if (( ${#path_display} > max_path )); then
            path_display="...${path_display[-$((max_path - 3)),-1]}"
        fi
        details="($path_display)"
        for flag in ${(s: :)_wt_completion_flags[idx]}; do
            case $flag in
                locked)   details="$details [${bold_yellow}locked${reset}]" ;;
                detached) details="$details [${dim}detached${reset}]" ;;
                prunable) details="$details [${prunable_color}prunable${reset}]" ;;
            esac
        done
        if [[ $b == "$current_branch" ]]; then
            branch_color="$current_color"
        else
            branch_color="$worktree_color"
        fi
        # reset before padding: \e[0m cancels menu-select highlight so it stops at the branch name
        descs+=("${branch_color}${b}${reset}${(r:$((max_branch-${#b})):):-}  $details")
    done
    compadd -l -d descs -- "${values[@]}"
}

_wt_path_branches() {
    _wt_complete_branches_with_paths
    _wt_collect_tags
    local -a detached_values detached_descs
    local idx tag_idx head tag
    _wt_setup_colors
    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        [[ -n ${_wt_completion_branches[idx]} ]] && continue
        head="${_wt_completion_heads[idx]}"
        for (( tag_idx = 1; tag_idx <= ${#_wt_tags[@]}; tag_idx++ )); do
            [[ ${_wt_tag_shas[tag_idx]} == "$head" ]] || continue
            tag="${_wt_tags[tag_idx]}"
            detached_values+=("$tag")
            detached_descs+=("${worktree_color}${tag}${reset}  (${dim_yellow}${head[1,8]}${reset}) [${dim}detached${reset}]")
        done
    done
    (( ${#detached_values[@]} > 0 )) && compadd -V detached -l -d detached_descs -- "${detached_values[@]}"
}

_wt_remove_targets() {
    local -a wt_values descs detached_values detached_descs
    local idx max_branch=0 details path_display branch_color current_branch="" b flag
    local cols=${COLUMNS:-0}
    local max_path=72
    _wt_setup_colors
    local -A seen_set
    local i w

    _wt_collect_worktree_rows || return 1

    for (( i = 1; i <= ${#words[@]}; i++ )); do
        [[ $i -eq $CURRENT ]] && continue
        w="${words[i]}"
        [[ $w == wt || $w == rm || $w == remove ]] && continue
        if [[ $w == --* || $w == -* ]]; then
            [[ $w == "--repo" ]] && (( i++ ))
            continue
        fi
        [[ -n $w ]] && seen_set[$w]=1
    done
    for (( idx = 1; idx <= ${#_wt_completion_paths[@]}; idx++ )); do
        (( ${+seen_set[${_wt_completion_paths[idx]}]} )) || continue
        [[ -n ${_wt_completion_branches[idx]} ]] && seen_set[${_wt_completion_branches[idx]}]=1
    done

    _wt_find_current_branch
    current_branch=$_wt_current_branch

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        b="${_wt_completion_branches[idx]}"
        [[ -z $b ]] && continue
        [[ ${_wt_completion_paths[idx]} == "$_wt_main_path" ]] && continue
        (( ${+seen_set[$b]} )) && continue
        wt_values+=("$b")
        (( ${#b} > max_branch )) && max_branch=${#b}
    done
    if (( cols > max_branch + 14 )); then
        max_path=$(( cols - max_branch - 10 ))
    fi
    (( max_path < 24 )) && max_path=24

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        b="${_wt_completion_branches[idx]}"
        [[ -z $b ]] && continue
        [[ ${_wt_completion_paths[idx]} == "$_wt_main_path" ]] && continue
        (( ${+seen_set[$b]} )) && continue
        path_display="${_wt_completion_paths[idx]/#${HOME}/~}"
        if (( ${#path_display} > max_path )); then
            path_display="...${path_display[-$((max_path - 3)),-1]}"
        fi
        details="($path_display)"
        for flag in ${(s: :)_wt_completion_flags[idx]}; do
            case $flag in
                locked)   details="$details [${bold_yellow}locked${reset}]" ;;
                detached) details="$details [${dim}detached${reset}]" ;;
                prunable) details="$details [${prunable_color}prunable${reset}]" ;;
            esac
        done
        if [[ $b == "$current_branch" ]]; then
            branch_color="$current_color"
        else
            branch_color="$worktree_color"
        fi
        # reset before padding: \e[0m cancels menu-select highlight so it stops at the branch name
        descs+=("${branch_color}${b}${reset}${(r:$((max_branch-${#b})):):-}  $details")
    done

    _wt_collect_tags
    local head tag tag_idx
    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        [[ -n ${_wt_completion_branches[idx]} ]] && continue
        [[ ${_wt_completion_paths[idx]} == "$_wt_main_path" ]] && continue
        head="${_wt_completion_heads[idx]}"
        for (( tag_idx = 1; tag_idx <= ${#_wt_tags[@]}; tag_idx++ )); do
            [[ ${_wt_tag_shas[tag_idx]} == "$head" ]] || continue
            tag="${_wt_tags[tag_idx]}"
            (( ${+seen_set[$tag]} )) && continue
            detached_values+=("$tag")
            detached_descs+=("${worktree_color}${tag}${reset}  (${dim_yellow}${head[1,8]}${reset}) [${dim}detached${reset}]")
        done
    done

    (( ${#wt_values[@]} > 0 )) && compadd -l -d descs -- "${wt_values[@]}"
    (( ${#detached_values[@]} > 0 )) && compadd -V detached -l -d detached_descs -- "${detached_values[@]}"
}

_wt_switch_targets() {
    local -A wt_set
    local -a wt_values wt_descs other_values other_descs
    local idx max_branch=0 details path_display branch branch_color current_branch="" b flag
    local cols=${COLUMNS:-0}
    local max_path=72
    _wt_setup_colors

    _wt_collect_worktree_rows
    _wt_collect_local_branches

    _wt_find_current_branch
    current_branch=$_wt_current_branch

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        b="${_wt_completion_branches[idx]}"
        [[ -z $b ]] && continue
        wt_set[$b]=1
        wt_values+=("$b")
        (( ${#b} > max_branch )) && max_branch=${#b}
    done
    for branch in "${_wt_local_branches[@]}"; do
        [[ -z $branch ]] && continue
        (( ${+wt_set[$branch]} )) && continue
        other_values+=("$branch")
        (( ${#branch} > max_branch )) && max_branch=${#branch}
    done
    if (( cols > max_branch + 14 )); then
        max_path=$(( cols - max_branch - 10 ))
    fi
    (( max_path < 24 )) && max_path=24

    for (( idx = 1; idx <= ${#_wt_completion_branches[@]}; idx++ )); do
        b="${_wt_completion_branches[idx]}"
        [[ -z $b ]] && continue
        path_display="${_wt_completion_paths[idx]/#${HOME}/~}"
        if (( ${#path_display} > max_path )); then
            path_display="...${path_display[-$((max_path - 3)),-1]}"
        fi
        details="($path_display)"
        for flag in ${(s: :)_wt_completion_flags[idx]}; do
            case $flag in
                locked)   details="$details [${bold_yellow}locked${reset}]" ;;
                detached) details="$details [${dim}detached${reset}]" ;;
                prunable) details="$details [${prunable_color}prunable${reset}]" ;;
            esac
        done
        if [[ $b == "$current_branch" ]]; then
            branch_color="$current_color"
        else
            branch_color="$worktree_color"
        fi
        # reset before padding: \e[0m cancels menu-select highlight so it stops at the branch name
        wt_descs+=("${branch_color}${b}${reset}${(r:$((max_branch-${#b})):):-}  $details")
    done
    for branch in "${other_values[@]}"; do
        other_descs+=("${dim}${branch}${reset}")
    done

    (( ${#wt_values[@]} > 0 )) && compadd -V worktrees -l -d wt_descs -- "${wt_values[@]}"
    (( ${#other_values[@]} > 0 )) && compadd -V branches -l -d other_descs -- "${other_values[@]}"
    (( ${#wt_values[@]} + ${#other_values[@]} > 0 ))
}

_wt_new_name() {
    local -A wt_set
    local -a results
    local i branch

    for (( i = 1; i <= ${#words[@]}; i++ )); do
        if [[ ${words[i]} == "-c" || ${words[i]} == "--create" ]]; then
            return 1
        fi
    done

    _wt_collect_worktree_rows
    _wt_collect_local_branches

    for (( i = 1; i <= ${#_wt_completion_branches[@]}; i++ )); do
        [[ -n ${_wt_completion_branches[i]} ]] && wt_set[${_wt_completion_branches[i]}]=1
    done

    for branch in "${_wt_local_branches[@]}"; do
        [[ -z $branch ]] && continue
        (( ${+wt_set[$branch]} )) && continue
        results+=("$branch")
    done

    (( ${#results[@]} > 0 )) && compadd -- "${results[@]}"
}

_wt_new_base() {
    _wt_collect_local_branches
    (( ${#_wt_local_branches[@]} > 0 )) && compadd -- "${_wt_local_branches[@]}"
}

_wt_prune_base() {
    _wt_collect_local_branches
    (( ${#_wt_local_branches[@]} > 0 )) && compadd -- "${_wt_local_branches[@]}"
}

_wt_link_files() {
    _wt_collect_worktree_rows || return 1
    _path_files -W "$_wt_main_path"
}

_wt_unlink_files() {
    local -A seen_set
    local i w
    for (( i = 1; i <= ${#words[@]}; i++ )); do
        [[ $i -eq $CURRENT ]] && continue
        w="${words[i]}"
        [[ $w == wt || $w == unlink ]] && continue
        [[ $w == --* || $w == -* ]] && continue
        [[ -n $w ]] && seen_set[$w]=1
    done

    _wt_collect_worktree_rows || return 1
    local primary_path
    primary_path=$(cd "$_wt_main_path" 2>/dev/null && pwd -P 2>/dev/null) || return 1

    local config="$HOME/.wt/config"
    [[ -f $config ]] || return 1

    local entry
    entry=$(grep -F "\"$primary_path\" = [" "$config" 2>/dev/null) || return 1
    local arr="${entry#*\" = \[}"
    arr="${arr%\]*}"

    local -a results
    local item
    for item in "${(@s:", ":)arr}"; do
        item="${item//\"/}"
        [[ -z $item ]] && continue
        (( ${+seen_set[$item]} )) && continue
        results+=("$item")
    done
    (( ${#results[@]} > 0 )) && compadd -- "${results[@]}"
}
"#;
        let dispatch_marker = "if [ \"$funcstack[1]\" = \"_wt\" ]; then";
        if let Some(idx) = script.find(dispatch_marker) {
            script.insert_str(idx, helper);
        } else {
            script.push_str(helper);
        }
        const LINK_FILES_TARGET: &str = "*::files -- Files or directories to link:_default";
        const UNLINK_FILES_TARGET: &str = "*::files -- Files or directories to unlink:_default";
        const PATH_NAME_TARGET: &str = ":name -- Branch name, tag, or ref:_default";
        const SWITCH_NAME_TARGET: &str = ":name -- Branch name:_default";
        const NEW_NAME_TARGET: &str = ":name -- Branch name or ref:_default";
        const NEW_BASE_TARGET: &str =
            "::base -- Start point for created branch (requires --create):_default";
        const NAMES_TARGET: &str = "*::names -- Branch names, refs, or paths:_default";
        const PRUNE_BASE_TARGET: &str =
            "--base=[Base branch for merged detection (e.g. develop, trunk)]:BASE:_default";
        for (label, target) in [
            ("link files", LINK_FILES_TARGET),
            ("unlink files", UNLINK_FILES_TARGET),
            ("path name", PATH_NAME_TARGET),
            ("switch name", SWITCH_NAME_TARGET),
            ("new name", NEW_NAME_TARGET),
            ("new base", NEW_BASE_TARGET),
            ("remove names", NAMES_TARGET),
            ("prune base", PRUNE_BASE_TARGET),
        ] {
            if !script.contains(target) {
                return Err(format!(
                    "cannot generate zsh completions: clap_complete output format changed \
                     ({label} target not found), please report this bug"
                ));
            }
        }
        script = script.replace(
            PATH_NAME_TARGET,
            ":name -- Branch name, tag, or ref:_wt_path_branches",
        );
        script = script.replace(
            SWITCH_NAME_TARGET,
            ":name -- Branch name:_wt_switch_targets",
        );
        script = script.replace(NEW_NAME_TARGET, ":name -- Branch name or ref:_wt_new_name");
        script = script.replace(
            NEW_BASE_TARGET,
            "::base -- Start point for created branch (requires --create):_wt_new_base",
        );
        script = script.replace(
            NAMES_TARGET,
            "*::names -- Branch names, refs, or paths:_wt_remove_targets",
        );
        script = script.replace(
            PRUNE_BASE_TARGET,
            "--base=[Base branch for merged detection (e.g. develop, trunk)]:BASE:_wt_prune_base",
        );
        script = script.replace(
            LINK_FILES_TARGET,
            "*::files -- Files or directories to link:_wt_link_files",
        );
        script = script.replace(
            UNLINK_FILES_TARGET,
            "*::files -- Files or directories to unlink:_wt_unlink_files",
        );
    }

    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_completion_is_dynamic() {
        let script = render(clap_complete::Shell::Zsh).unwrap();
        for func in [
            "_wt_extract_repo_args()",
            "_wt_collect_worktree_rows()",
            "_wt_collect_local_branches()",
            "_wt_collect_tags()",
            "_wt_setup_colors()",
            "_wt_find_current_branch()",
            "_wt_complete_branches_with_paths()",
            "_wt_path_branches()",
            "_wt_remove_targets()",
            "_wt_switch_targets()",
            "_wt_new_name()",
            "_wt_new_base()",
            "_wt_prune_base()",
            "_wt_link_files()",
            "_wt_unlink_files()",
        ] {
            assert!(script.contains(func), "missing helper: {func}");
        }
        let dispatch = script
            .find("if [ \"$funcstack[1]\" = \"_wt\" ]; then")
            .unwrap();
        assert!(script.find("_wt_path_branches()").unwrap() < dispatch);
        assert_eq!(
            script
                .matches(":name -- Branch name, tag, or ref:_wt_path_branches")
                .count(),
            2
        );
        assert_eq!(
            script
                .matches(":name -- Branch name:_wt_switch_targets")
                .count(),
            2
        );
        assert_eq!(
            script
                .matches(":name -- Branch name or ref:_wt_new_name")
                .count(),
            2
        );
        assert_eq!(
            script
                .matches(
                    "::base -- Start point for created branch (requires --create):_wt_new_base"
                )
                .count(),
            2
        );
        assert_eq!(
            script
                .matches("*::names -- Branch names, refs, or paths:_wt_remove_targets")
                .count(),
            2
        );
        assert_eq!(
            script
                .matches(
                    "--base=[Base branch for merged detection (e.g. develop, trunk)]:BASE:_wt_prune_base"
                )
                .count(),
            1
        );
        assert!(!script.contains("Branch name, tag, or ref:_default"));
        assert!(!script.contains("Branch name:_default"));
        assert!(!script.contains("Branch name or ref:_default"));
        assert!(!script.contains("Start point for created branch (requires --create):_default"));
        assert!(!script.contains("Branch names, refs, or paths:_default"));
        assert!(
            !script
                .contains("Base branch for merged detection (e.g. develop, trunk)]:BASE:_default")
        );
    }

    #[test]
    fn zsh_link_unlink_completions_are_dynamic() {
        let script = render(clap_complete::Shell::Zsh).unwrap();
        assert!(script.contains("_wt_link_files()"));
        assert!(script.contains("_wt_unlink_files()"));
        assert!(!script.contains("Files or directories to link:_default"));
        assert!(!script.contains("Files or directories to unlink:_default"));
        assert_eq!(
            script
                .matches("*::files -- Files or directories to link:_wt_link_files")
                .count(),
            2
        );
        assert_eq!(
            script
                .matches("*::files -- Files or directories to unlink:_wt_unlink_files")
                .count(),
            1
        );
    }

    #[test]
    fn bash_completion_does_not_include_zsh_helper() {
        let script = render(clap_complete::Shell::Bash).unwrap();
        assert!(!script.contains("_wt_path_branches()"));
        assert!(!script.contains("_wt_switch_targets()"));
        assert!(!script.contains("_wt_new_name()"));
        assert!(!script.contains("_wt_new_base()"));
        assert!(!script.contains("_wt_link_files()"));
        assert!(!script.contains("_wt_unlink_files()"));
    }
}
