use std::fs;
use std::path::{Path, PathBuf};

use crate::git::Git;

pub fn run(dry_run: bool) -> Result<(), String> {
    if let Ok(repo_root) = Git::find_repo(None) {
        let git = Git::new(&repo_root);
        git.prune_worktrees(dry_run)?;
    }

    let home = std::env::var("HOME").map_err(|_| "$HOME is not set".to_string())?;
    let wt_root = Path::new(&home).join(".worktrees");

    if !wt_root.is_dir() {
        return Ok(());
    }

    let orphans = find_orphans(&wt_root);

    if orphans.is_empty() {
        return Ok(());
    }

    if dry_run {
        for orphan in &orphans {
            println!("{}", orphan.display());
        }
        eprintln!(
            "wt: would remove {} orphaned worktree(s) (dry run)",
            orphans.len()
        );
    } else {
        for orphan in &orphans {
            fs::remove_dir_all(orphan)
                .map_err(|e| format!("cannot remove {}: {e}", orphan.display()))?;
            eprintln!("wt: removed {}", orphan.display());
        }
        cleanup_empty_parents(&orphans, &wt_root);
        eprintln!("wt: removed {} orphaned worktree(s)", orphans.len());
    }

    Ok(())
}

fn find_orphans(wt_root: &Path) -> Vec<PathBuf> {
    let mut orphans = Vec::new();
    scan_dir(wt_root, &mut orphans);
    orphans
}

fn scan_dir(dir: &Path, orphans: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("wt: cannot read directory {}: {e}", dir.display());
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("wt: cannot read entry in {}: {e}", dir.display());
                continue;
            }
        };

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();

        let dot_git = path.join(".git");

        if dot_git.is_file() {
            if let Some(gitdir) = parse_gitdir(&dot_git) {
                if !gitdir.exists() {
                    orphans.push(path);
                }
            } else {
                eprintln!("wt: warning: cannot parse {}, skipping", dot_git.display());
            }
        } else if !dot_git.is_dir() {
            scan_dir(&path, orphans);
        }
    }
}

fn parse_gitdir(dot_git_file: &Path) -> Option<PathBuf> {
    let content = fs::read_to_string(dot_git_file).ok()?;
    let line = content.lines().next()?;
    let gitdir = line.strip_prefix("gitdir: ")?.trim();
    if gitdir.is_empty() {
        return None;
    }
    let gitdir_path = PathBuf::from(gitdir);

    if gitdir_path.is_absolute() {
        Some(gitdir_path)
    } else {
        let parent = dot_git_file.parent()?;
        Some(parent.join(gitdir_path))
    }
}

fn cleanup_empty_parents(orphans: &[PathBuf], wt_root: &Path) {
    let mut candidates: Vec<&Path> = orphans.iter().filter_map(|p| p.parent()).collect();

    candidates.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    candidates.dedup();

    for dir in candidates {
        cleanup_dir_chain(dir, wt_root);
    }
}

fn cleanup_dir_chain(mut dir: &Path, wt_root: &Path) {
    while dir != wt_root && dir.starts_with(wt_root) {
        let is_empty = fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none());
        if !is_empty {
            break;
        }
        if fs::remove_dir(dir).is_err() {
            break;
        }
        eprintln!("wt: removed empty directory {}", dir.display());
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
}
