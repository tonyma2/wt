use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::git::Git;
use crate::terminal::{self, Colors, trunc, trunc_tail};
use crate::worktree::{self, WorktreeInfo};

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

    let output = Git::new(&repo_root).list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);
    let cwd = resolve_cwd();
    let current_path = worktree::find_current_worktree(&worktrees, cwd.as_deref());

    let infos = worktree::enrich_worktrees(&worktrees, current_path.as_deref());

    if json {
        let entries = build_json_entries(&infos, None);
        let json_str =
            serde_json::to_string(&entries).map_err(|e| format!("cannot serialize json: {e}"))?;
        println!("{json_str}");
        return Ok(());
    }

    let cols = terminal::width();
    let clr = terminal::colors();
    print_table(&infos, cols, &clr, "");

    Ok(())
}

fn run_all(json: bool) -> Result<(), String> {
    let repos = worktree::load_all()?;
    if repos.is_empty() {
        if json {
            println!("[]");
        }
        return Ok(());
    }

    if json {
        let entries: Vec<_> = repos
            .iter()
            .flat_map(|repo| build_json_entries(&repo.worktrees, Some(&repo.name)))
            .collect();
        let json_str =
            serde_json::to_string(&entries).map_err(|e| format!("cannot serialize json: {e}"))?;
        println!("{json_str}");
    } else {
        let cols = terminal::width();
        let clr = terminal::colors();
        for (i, repo) in repos.iter().enumerate() {
            if i > 0 {
                println!();
            }
            println!("{}{}:{}", clr.bold, repo.name, clr.reset);
            print_table(&repo.worktrees, cols, &clr, "  ");
        }
    }

    Ok(())
}

fn resolve_cwd() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok())
}

fn build_json_entries(worktrees: &[WorktreeInfo], repo_name: Option<&str>) -> Vec<WorktreeEntry> {
    worktrees
        .iter()
        .map(|wt| {
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
                dirty: wt.dirty,
                ahead: wt.ahead,
                behind: wt.behind,
                current: wt.current,
            }
        })
        .collect()
}

fn print_table(worktrees: &[WorktreeInfo], cols: usize, clr: &Colors, indent: &str) {
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
        let branch = wt
            .branch
            .as_deref()
            .unwrap_or(if wt.bare { "(bare)" } else { "(detached)" });
        let branch_trunc = trunc(branch, branch_w);

        let status = worktree::format_status(wt.bare, wt.dirty, wt.ahead, wt.behind);
        let status_trunc = trunc(&status, status_w);

        let path_str = terminal::tilde_path(&wt.path);
        let path_trunc = trunc_tail(&path_str, path_w);

        let badges = worktree_badges(wt, clr);

        let branch_pad = branch_w.saturating_sub(branch_trunc.chars().count());
        let branch_color = if wt.current { clr.green } else { "" };
        let branch_col = format!(
            "{}{}{}{}",
            branch_color,
            branch_trunc,
            clr.reset,
            " ".repeat(branch_pad)
        );

        let cur_col = if wt.current { "*" } else { " " };

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

fn worktree_badges(wt: &WorktreeInfo, clr: &Colors) -> String {
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
