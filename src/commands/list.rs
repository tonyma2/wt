use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::git::Git;
use crate::terminal::{self, Colors, trunc, trunc_tail};
use crate::worktree::{self, Worktree};

#[derive(Serialize)]
struct WorktreeEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    repo: Option<String>,
    name: String,
    path: String,
    branch: Option<String>,
    head: String,
    bare: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
    dirty: bool,
    ahead: Option<u64>,
    behind: Option<u64>,
    current: bool,
}

pub fn run(repo: Option<&Path>, json: bool, all: bool) -> Result<(), String> {
    if all {
        return run_all(json);
    }

    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);
    let cwd = resolve_cwd();
    let current_path = find_current(&worktrees, cwd.as_deref());

    if json {
        let entries = build_json_entries(&git, &worktrees, current_path, None);
        let json_str =
            serde_json::to_string(&entries).map_err(|e| format!("cannot serialize json: {e}"))?;
        println!("{json_str}");
        return Ok(());
    }

    let cols = terminal::width();
    let clr = terminal::colors();
    print_table(&git, &worktrees, current_path, cols, &clr, "");

    Ok(())
}

fn run_all(json: bool) -> Result<(), String> {
    let wt_root = worktree::worktrees_root()?;

    if !wt_root.is_dir() {
        if json {
            println!("[]");
        }
        return Ok(());
    }
    let wt_root = worktree::canonicalize_or_self(&wt_root);
    let repos = worktree::discover_repos(&wt_root);

    if repos.is_empty() {
        if json {
            println!("[]");
        }
        return Ok(());
    }

    let cwd = resolve_cwd();

    let err_clr = terminal::stderr_colors();
    let repo_data: Vec<_> = repos
        .iter()
        .filter_map(|repo_path| {
            let git = Git::new(repo_path);
            let output = match git.list_worktrees() {
                Ok(o) => o,
                Err(e) => {
                    eprintln!(
                        "{}cannot list {}: {e}{}",
                        err_clr.red,
                        repo_path.display(),
                        err_clr.reset
                    );
                    return None;
                }
            };
            let worktrees = worktree::parse_porcelain(&output);
            let name = worktree::repo_basename(repo_path);
            Some((name, git, worktrees))
        })
        .collect();

    if json {
        let mut all_entries = Vec::new();
        for (name, git, worktrees) in &repo_data {
            let current_path = find_current(worktrees, cwd.as_deref());
            all_entries.extend(build_json_entries(git, worktrees, current_path, Some(name)));
        }
        let json_str = serde_json::to_string(&all_entries)
            .map_err(|e| format!("cannot serialize json: {e}"))?;
        println!("{json_str}");
    } else {
        let cols = terminal::width();
        let clr = terminal::colors();
        for (i, (name, git, worktrees)) in repo_data.iter().enumerate() {
            let current_path = find_current(worktrees, cwd.as_deref());
            if i > 0 {
                println!();
            }
            println!("{}{}:{}", clr.bold, name, clr.reset);
            print_table(git, worktrees, current_path, cols, &clr, "  ");
        }
    }

    Ok(())
}

fn resolve_cwd() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok())
}

fn find_current<'a>(worktrees: &'a [Worktree], cwd: Option<&Path>) -> Option<&'a Path> {
    let cwd = cwd?;
    worktrees
        .iter()
        .filter(|wt| !wt.prunable)
        .filter_map(|wt| {
            let canonical = worktree::canonicalize_or_self(&wt.path);
            cwd.starts_with(&canonical)
                .then_some((wt.path.as_path(), canonical))
        })
        .max_by_key(|(_, canonical)| canonical.components().count())
        .map(|(path, _)| path)
}

fn build_json_entries(
    git: &Git,
    worktrees: &[Worktree],
    current_path: Option<&Path>,
    repo_name: Option<&str>,
) -> Vec<WorktreeEntry> {
    worktrees
        .iter()
        .map(|wt| {
            let is_current = current_path == Some(wt.path.as_path());
            let (dirty, ahead, behind) = computed_status(git, wt);
            let path = wt.path.to_string_lossy().into_owned();
            let branch = wt.branch.clone();
            let name = branch.clone().unwrap_or_else(|| path.clone());
            WorktreeEntry {
                repo: repo_name.map(|s| s.to_string()),
                name,
                path,
                branch,
                head: wt.head.clone(),
                bare: wt.bare,
                detached: wt.detached,
                locked: wt.locked,
                prunable: wt.prunable,
                dirty,
                ahead,
                behind,
                current: is_current,
            }
        })
        .collect()
}

fn print_table(
    git: &Git,
    worktrees: &[Worktree],
    current_path: Option<&Path>,
    cols: usize,
    clr: &Colors,
    indent: &str,
) {
    let cur_w: usize = 1;
    let branch_min: usize = 14;
    let branch_max: usize = 24;
    let status_w: usize = 10;
    let path_min: usize = 24;
    let indent_w = indent.len();
    let avail = cols.saturating_sub(indent_w + cur_w + status_w + 7);

    let extra = avail.saturating_sub(path_min + branch_min);
    let branch_w = (branch_min + extra / 8).min(branch_max);
    let path_w = avail.saturating_sub(branch_w);

    println!(
        "{indent}{:<cur_w$} {:<branch_w$}   {:<status_w$}   PATH",
        "", "BRANCH", "STATUS",
    );

    for wt in worktrees {
        let is_current = current_path == Some(wt.path.as_path());

        let branch = wt
            .branch
            .as_deref()
            .unwrap_or(if wt.bare { "(bare)" } else { "(detached)" });
        let branch_trunc = trunc(branch, branch_w);

        let (dirty, ahead, behind) = computed_status(git, wt);
        let status = format_status(wt.bare, dirty, ahead, behind);
        let status_trunc = trunc(&status, status_w);

        let path_str = terminal::tilde_path(&wt.path);
        let path_trunc = trunc_tail(&path_str, path_w);

        let badges = worktree_badges(wt, clr);

        let branch_pad = branch_w.saturating_sub(branch_trunc.chars().count());
        let branch_color = if is_current { clr.green } else { "" };
        let branch_col = format!(
            "{}{}{}{}",
            branch_color,
            branch_trunc,
            clr.reset,
            " ".repeat(branch_pad)
        );

        let cur_col = if is_current { "*" } else { " " };

        let row_suffix = if badges.is_empty() {
            path_trunc
        } else {
            format!("{path_trunc}  {badges}")
        };

        println!(
            "{indent}{cur_col} {branch_col}   {:<status_w$}   {row_suffix}",
            status_trunc,
        );
    }
}

fn computed_status(git: &Git, wt: &Worktree) -> (bool, Option<u64>, Option<u64>) {
    if wt.bare || wt.prunable {
        return (false, None, None);
    }
    let dirty = git.is_dirty(&wt.path);
    let (ahead, behind) = wt
        .branch
        .as_deref()
        .and_then(|b| git.ahead_behind(b))
        .map_or((None, None), |(a, b)| (Some(a), Some(b)));
    (dirty, ahead, behind)
}

fn format_status(bare: bool, dirty: bool, ahead: Option<u64>, behind: Option<u64>) -> String {
    if bare {
        return "bare".into();
    }
    let mut parts: Vec<String> = Vec::new();
    if dirty {
        parts.push("*".into());
    }
    if let Some(a) = ahead
        && a > 0
    {
        parts.push(format!("↑{a}"));
    }
    if let Some(b) = behind
        && b > 0
    {
        parts.push(format!("↓{b}"));
    }
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(" ")
    }
}

fn worktree_badges(wt: &Worktree, clr: &Colors) -> String {
    if wt.bare {
        return String::new();
    }
    let mut badges = Vec::new();
    if wt.detached {
        badges.push(format!("{}[detached]{}", clr.dim, clr.reset));
    }
    if wt.locked {
        badges.push(format!("{}[locked]{}", clr.bold_yellow, clr.reset));
    }
    if wt.prunable {
        badges.push(format!("{}[prunable]{}", clr.red, clr.reset));
    }
    badges.join(" ")
}
