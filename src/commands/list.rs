use std::path::Path;

use crate::git::Git;
use crate::terminal::{self, Colors};
use crate::worktree::{self, Worktree};

pub fn run(repo: Option<&Path>, porcelain: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    let output = git.list_worktrees()?;

    if porcelain {
        print!("{output}");
        return Ok(());
    }

    let worktrees = worktree::parse_porcelain(&output);
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok());

    let current_path = cwd.as_deref().and_then(|cwd| {
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
    });

    let cols = terminal::width();
    let clr = terminal::colors();

    let cur_w: usize = 1;
    let branch_min: usize = 14;
    let branch_max: usize = 24;
    let status_w: usize = 10;
    let path_min: usize = 24;
    let avail = cols.saturating_sub(cur_w + status_w + 7);

    let (branch_w, path_w) = if avail <= path_min + branch_min {
        let bw = branch_min;
        let pw = avail.saturating_sub(bw).max(12);
        (bw, pw)
    } else {
        let extra = avail - path_min - branch_min;
        let branch_extra = extra / 8;
        let bw = (branch_min + branch_extra).min(branch_max);
        let pw = avail - bw;
        (bw, pw)
    };

    println!(
        "{:<cur_w$} {:<branch_w$}   {:<status_w$}   PATH",
        "", "BRANCH", "STATUS",
    );

    for wt in &worktrees {
        let is_current = current_path == Some(wt.path.as_path());

        let branch = wt.branch.as_deref().unwrap_or("(detached)");
        let branch_trunc = trunc(branch, branch_w);

        let status = worktree_status(&git, wt);
        let status_trunc = trunc(&status, status_w);

        let path_str = terminal::tilde_path(&wt.path);
        let path_trunc = trunc_tail(&path_str, path_w);

        let badges = worktree_badges(wt, &clr);

        let branch_pad = branch_w.saturating_sub(branch_trunc.chars().count());
        let branch_color = if is_current { clr.green } else { clr.cyan };
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
            "{cur_col} {branch_col}   {:<status_w$}   {row_suffix}",
            status_trunc,
        );
    }

    Ok(())
}

fn worktree_status(git: &Git, wt: &Worktree) -> String {
    if wt.bare {
        return "bare".into();
    }
    if wt.prunable {
        return "-".into();
    }

    let mut parts: Vec<String> = Vec::new();
    if git.is_dirty(&wt.path) {
        parts.push("*".into());
    }
    if let Some(branch) = &wt.branch
        && let Some((ahead, behind)) = git.ahead_behind(branch)
    {
        if ahead > 0 {
            parts.push(format!("↑{ahead}"));
        }
        if behind > 0 {
            parts.push(format!("↓{behind}"));
        }
    }

    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(" ")
    }
}

fn worktree_badges(wt: &Worktree, clr: &Colors) -> String {
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

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max <= 3 {
        return s.chars().take(max).collect();
    }
    let end = s.char_indices().nth(max - 3).map_or(s.len(), |(i, _)| i);
    format!("{}...", &s[..end])
}

fn trunc_tail(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        return s.to_string();
    }
    if max <= 3 {
        let start = s.char_indices().nth(char_count - max).map_or(0, |(i, _)| i);
        return s[start..].to_string();
    }
    let start = s
        .char_indices()
        .nth(char_count - max + 3)
        .map_or(0, |(i, _)| i);
    format!("...{}", &s[start..])
}
