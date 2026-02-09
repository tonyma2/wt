use clap::CommandFactory;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::{Cli, Shell};

pub fn run(shell_arg: Option<Shell>) -> Result<(), String> {
    let shell = resolve_shell(shell_arg, std::env::var_os("SHELL").as_deref())?;
    let target = completion_path(
        shell,
        home_dir().as_deref(),
        std::env::var_os("XDG_DATA_HOME").as_deref(),
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
    )?;

    let dir = target
        .parent()
        .ok_or_else(|| format!("cannot determine parent directory for {}", target.display()))?;
    fs::create_dir_all(dir)
        .map_err(|e| format!("cannot create directory {}: {e}", dir.display()))?;

    let script = render(shell);
    let state = install_script(&target, script.as_bytes())?;
    print_status(state, shell, &target, dir);
    println!("{}", target.display());
    Ok(())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

fn resolve_shell(
    shell_arg: Option<Shell>,
    shell_env: Option<&OsStr>,
) -> Result<Shell, String> {
    shell_arg
        .or_else(|| detect_shell(shell_env))
        .ok_or_else(|| "cannot detect supported shell; use --shell zsh|bash|fish".to_string())
}

fn detect_shell(shell_env: Option<&OsStr>) -> Option<Shell> {
    let shell = shell_env?;
    let name = Path::new(shell)
        .file_name()
        .and_then(OsStr::to_str)
        .or_else(|| shell.to_str())
        .unwrap_or("")
        .trim()
        .trim_start_matches('-');

    match name {
        "zsh" => Some(Shell::Zsh),
        "bash" => Some(Shell::Bash),
        "fish" => Some(Shell::Fish),
        _ => None,
    }
}

fn completion_path(
    shell: Shell,
    home: Option<&Path>,
    xdg_data_home: Option<&OsStr>,
    xdg_config_home: Option<&OsStr>,
) -> Result<PathBuf, String> {
    match shell {
        Shell::Zsh => {
            Ok(xdg_data_dir(home, xdg_data_home)?.join("zsh/site-functions/_wt"))
        }
        Shell::Bash => {
            Ok(xdg_data_dir(home, xdg_data_home)?.join("bash-completion/completions/wt"))
        }
        Shell::Fish => {
            Ok(xdg_config_dir(home, xdg_config_home)?.join("fish/completions/wt.fish"))
        }
    }
}

fn xdg_data_dir(home: Option<&Path>, xdg_data_home: Option<&OsStr>) -> Result<PathBuf, String> {
    if let Some(path) = xdg_data_home.filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("XDG_DATA_HOME must be an absolute path".to_string());
        }
        return Ok(path);
    }
    let home =
        home.ok_or_else(|| "home directory is not set; set $HOME or XDG_DATA_HOME".to_string())?;
    Ok(home.join(".local/share"))
}

fn xdg_config_dir(home: Option<&Path>, xdg_config_home: Option<&OsStr>) -> Result<PathBuf, String> {
    if let Some(path) = xdg_config_home.filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("XDG_CONFIG_HOME must be an absolute path".to_string());
        }
        return Ok(path);
    }
    let home =
        home.ok_or_else(|| "home directory is not set; set $HOME or XDG_CONFIG_HOME".to_string())?;
    Ok(home.join(".config"))
}

fn render(shell: Shell) -> String {
    let mut out = Vec::new();
    clap_complete::generate(shell_to_clap(shell), &mut Cli::command(), "wt", &mut out);
    let mut script = String::from_utf8_lossy(&out).into_owned();

    if shell == Shell::Zsh {
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

fn shell_to_clap(shell: Shell) -> clap_complete::Shell {
    match shell {
        Shell::Zsh => clap_complete::Shell::Zsh,
        Shell::Bash => clap_complete::Shell::Bash,
        Shell::Fish => clap_complete::Shell::Fish,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstallState {
    Installed,
    Updated,
    Unchanged,
}

fn install_script(path: &Path, desired: &[u8]) -> Result<InstallState, String> {
    let state = match fs::read(path) {
        Ok(existing) if existing == desired => InstallState::Unchanged,
        Ok(_) => InstallState::Updated,
        Err(e) if e.kind() == io::ErrorKind::NotFound => InstallState::Installed,
        Err(e) => {
            return Err(format!(
                "cannot read completion file {}: {e}",
                path.display()
            ));
        }
    };

    if state != InstallState::Unchanged {
        write_atomic(path, desired)?;
    }

    Ok(state)
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("cannot determine parent directory for {}", path.display()))?;
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or("wt");
    let tmp = dir.join(format!(".{name}.tmp.{}", std::process::id()));

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
        .map_err(|e| format!("cannot create temporary file in {}: {e}", dir.display()))?;

    if let Err(e) = file.write_all(data).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "cannot write completion file {}: {e}",
            path.display()
        ));
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "cannot write completion file {}: {e}",
            path.display()
        ));
    }

    Ok(())
}

fn print_status(state: InstallState, shell: Shell, target: &Path, dir: &Path) {
    match state {
        InstallState::Installed => {
            eprintln!("wt: installed completion file at {}", target.display());
        }
        InstallState::Updated => {
            eprintln!("wt: updated completion file at {}", target.display());
        }
        InstallState::Unchanged => {
            eprintln!(
                "wt: completion file is already up to date at {}",
                target.display()
            );
            return;
        }
    }

    match shell {
        Shell::Zsh => {
            eprintln!("wt: add this to ~/.zshrc");
            eprintln!("wt: fpath=(\"{}\" $fpath)", dir.display());
            eprintln!("wt: autoload -Uz compinit && compinit");
        }
        Shell::Bash => {
            eprintln!(
                "wt: add this to your bash startup file (for example ~/.bashrc or ~/.bash_profile)"
            );
            eprintln!("wt: if [ -f \"{}\" ]; then", target.display());
            eprintln!("wt:   source \"{}\"", target.display());
            eprintln!("wt: fi");
        }
        Shell::Fish => {
            eprintln!(
                "wt: fish loads completions from {} automatically",
                dir.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_shell_from_env() {
        assert_eq!(
            resolve_shell(None, Some(OsStr::new("/bin/zsh"))).unwrap(),
            Shell::Zsh
        );
        assert_eq!(
            resolve_shell(None, Some(OsStr::new("/usr/bin/bash"))).unwrap(),
            Shell::Bash
        );
        assert_eq!(
            resolve_shell(None, Some(OsStr::new("/opt/homebrew/bin/fish"))).unwrap(),
            Shell::Fish
        );
        assert_eq!(
            resolve_shell(None, Some(OsStr::new("/bin/-zsh"))).unwrap(),
            Shell::Zsh
        );
    }

    #[test]
    fn explicit_shell_overrides_env() {
        assert_eq!(
            resolve_shell(Some(Shell::Fish), Some(OsStr::new("/bin/zsh"))).unwrap(),
            Shell::Fish
        );
    }

    #[test]
    fn unsupported_shell_returns_actionable_error() {
        let err = resolve_shell(None, Some(OsStr::new("/bin/tcsh"))).unwrap_err();
        assert_eq!(
            err,
            "cannot detect supported shell; use --shell zsh|bash|fish"
        );
    }

    #[test]
    fn rejects_relative_xdg_paths() {
        let err = completion_path(
            Shell::Zsh,
            Some(Path::new("/home/test")),
            Some(OsStr::new("relative/data")),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "XDG_DATA_HOME must be an absolute path");

        let err = completion_path(
            Shell::Fish,
            Some(Path::new("/home/test")),
            None,
            Some(OsStr::new("relative/config")),
        )
        .unwrap_err();
        assert_eq!(err, "XDG_CONFIG_HOME must be an absolute path");
    }

    #[test]
    fn path_resolution_prefers_xdg_directories() {
        let home = Path::new("/home/test");
        assert_eq!(
            completion_path(
                Shell::Zsh,
                Some(home),
                Some(OsStr::new("/xdg/data")),
                Some(OsStr::new("/xdg/config"))
            )
            .unwrap(),
            PathBuf::from("/xdg/data/zsh/site-functions/_wt")
        );
        assert_eq!(
            completion_path(
                Shell::Bash,
                Some(home),
                Some(OsStr::new("/xdg/data")),
                Some(OsStr::new("/xdg/config"))
            )
            .unwrap(),
            PathBuf::from("/xdg/data/bash-completion/completions/wt")
        );
        assert_eq!(
            completion_path(
                Shell::Fish,
                Some(home),
                Some(OsStr::new("/xdg/data")),
                Some(OsStr::new("/xdg/config"))
            )
            .unwrap(),
            PathBuf::from("/xdg/config/fish/completions/wt.fish")
        );
    }

    #[test]
    fn requires_home_only_for_missing_xdg_fallbacks() {
        assert_eq!(
            completion_path(
                Shell::Fish,
                None,
                None,
                Some(OsStr::new("/xdg/config"))
            )
            .unwrap(),
            PathBuf::from("/xdg/config/fish/completions/wt.fish")
        );

        let err = completion_path(Shell::Zsh, None, None, None).unwrap_err();
        assert_eq!(err, "home directory is not set; set $HOME or XDG_DATA_HOME");
    }

    #[test]
    fn zsh_path_completion_is_dynamic() {
        let script = render(Shell::Zsh);
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
        let script = render(Shell::Bash);
        assert!(!script.contains("_wt_path_branches()"));
    }
}
