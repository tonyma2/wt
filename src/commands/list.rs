use std::fmt::Write;
use std::path::Path;

use crate::git::Git;
use crate::terminal;
use crate::worktree::{self, Worktree};

pub fn run(repo: Option<&Path>, porcelain: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    if porcelain {
        let output = git.list_worktrees()?;
        print!("{output}");
        return Ok(());
    }

    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok());

    let cols = terminal::width();

    let cur_w: usize = 1;
    let branch_min: usize = 14;
    let branch_max: usize = 24;
    let head_w: usize = 8;
    let flags_w: usize = 8;
    let path_min: usize = 24;
    let avail = cols.saturating_sub(cur_w + head_w + flags_w + 11);

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
        "{:<cur_w$}  {:<branch_w$}  {:<head_w$}  {:<flags_w$}  PATH",
        "", "BRANCH", "HEAD", "STATE",
    );

    for wt in &worktrees {
        let is_current = cwd.as_ref().is_some_and(|c| {
            let wt_canon = std::fs::canonicalize(&wt.path).unwrap_or_else(|_| wt.path.clone());
            c == &wt_canon || c.starts_with(&wt_canon)
        });
        let cur_marker = if is_current { "*" } else { "" };

        let branch = wt.branch.as_deref().unwrap_or("(detached)");
        let branch_trunc = trunc(branch, branch_w);

        let head_trunc = if wt.head.bytes().all(|b| b == b'0') {
            "-".to_string()
        } else if wt.head.len() > head_w {
            wt.head[..head_w].to_string()
        } else {
            wt.head.clone()
        };
        let status = worktree_status(&git, wt);
        let flags_trunc = trunc(&status, flags_w);
        let path_str = wt.path.to_string_lossy();
        let path_trunc = trunc_tail(&path_str, path_w);

        println!(
            "{:<cur_w$}  {:<branch_w$}  {:<head_w$}  {:<flags_w$}  {}",
            cur_marker, branch_trunc, head_trunc, flags_trunc, path_trunc,
        );
    }

    Ok(())
}

fn worktree_status(git: &Git, wt: &Worktree) -> String {
    if wt.bare {
        return "bare".into();
    }

    let mut s = String::new();
    if git.is_dirty(&wt.path) {
        s.push('*');
    }
    if let Some(branch) = &wt.branch
        && let Some((ahead, behind)) = git.ahead_behind(branch)
    {
        if ahead > 0 {
            write!(s, "+{ahead}").unwrap();
        }
        if behind > 0 {
            write!(s, "-{behind}").unwrap();
        }
    }

    let mut flags = Vec::new();
    if wt.detached {
        flags.push("detached");
    }
    if wt.locked {
        flags.push("locked");
    }
    if wt.prunable {
        flags.push("prunable");
    }
    if !s.is_empty() && !flags.is_empty() {
        s.push(',');
    }
    s.push_str(&flags.join(","));

    if s.is_empty() { "-".into() } else { s }
}

fn trunc(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else if max <= 3 {
        chars[..max].iter().collect()
    } else {
        let mut out: String = chars[..max - 3].iter().collect();
        out.push_str("...");
        out
    }
}

fn trunc_tail(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else if max <= 3 {
        chars[chars.len() - max..].iter().collect()
    } else {
        let mut out = String::from("...");
        out.extend(&chars[chars.len() - max + 3..]);
        out
    }
}
