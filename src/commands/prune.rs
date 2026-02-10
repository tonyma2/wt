use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::worktree::parse_porcelain;

pub fn run(dry_run: bool, gone: bool, repo: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().and_then(|p| p.canonicalize()).ok();

    if let Some(repo_path) = repo {
        let repo_root = Git::find_repo(Some(repo_path))?;
        let git = Git::new(&repo_root);
        let output = git.prune_worktrees(dry_run)?;
        if !output.is_empty() {
            eprintln!("{output}");
        }
        prune_merged(&git, dry_run, gone, &cwd, None)?;
        return Ok(());
    }

    let home = std::env::var("HOME").map_err(|_| "$HOME is not set".to_string())?;
    let wt_root = Path::new(&home).join(".worktrees");

    if !wt_root.is_dir() {
        return Ok(());
    }
    let wt_root = fs::canonicalize(&wt_root).unwrap_or(wt_root);

    let repos = discover_repos(&wt_root);
    let mut errors = 0usize;
    for repo_path in &repos {
        if !repo_path.exists() {
            continue;
        }
        let git = Git::new(repo_path);
        match git.prune_worktrees(dry_run) {
            Ok(output) if !output.is_empty() => {
                eprintln!("wt: pruning {}", repo_path.display());
                eprintln!("{output}");
            }
            Err(e) => {
                eprintln!("wt: cannot prune {}: {e}", repo_path.display());
                errors += 1;
                continue;
            }
            _ => {}
        }
        if let Err(e) = prune_merged(&git, dry_run, gone, &cwd, Some(&wt_root)) {
            eprintln!("wt: cannot prune merged in {}: {e}", repo_path.display());
            errors += 1;
        }
    }

    let orphans = find_orphans(&wt_root);

    if !orphans.is_empty() {
        let orphans: Vec<PathBuf> = orphans
            .into_iter()
            .filter(|orphan| {
                if let Some(cwd) = &cwd
                    && let Ok(canonical) = orphan.canonicalize()
                    && (cwd == &canonical || cwd.starts_with(&canonical))
                {
                    let label = orphan.strip_prefix(&wt_root).unwrap_or(orphan.as_path());
                    eprintln!(
                        "wt: skipping {} (orphan, current directory)",
                        label.display()
                    );
                    return false;
                }
                true
            })
            .collect();

        if dry_run {
            for orphan in &orphans {
                println!("{}", orphan.display());
            }
        } else {
            for orphan in &orphans {
                fs::remove_dir_all(orphan)
                    .map_err(|e| format!("cannot remove {}: {e}", orphan.display()))?;
                let label = orphan.strip_prefix(&wt_root).unwrap_or(orphan.as_path());
                eprintln!("wt: removed {} (orphan)", label.display());
            }
            cleanup_empty_parents(&orphans, &wt_root);
        }
    }

    if errors > 0 {
        return Err(format!("cannot prune {} repo(s)", errors));
    }

    Ok(())
}

fn discover_repos(wt_root: &Path) -> BTreeSet<PathBuf> {
    let mut repos = BTreeSet::new();
    collect_repos(wt_root, &mut repos);
    repos
}

fn collect_repos(dir: &Path, repos: &mut BTreeSet<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if !ft.is_dir() {
            continue;
        }

        let path = entry.path();
        let dot_git = path.join(".git");

        if dot_git.is_file() {
            if let Some(gitdir) = parse_gitdir(&dot_git)
                && let Some(admin) = admin_repo_from_gitdir(&gitdir)
            {
                repos.insert(admin);
            }
        } else if !dot_git.is_dir() {
            collect_repos(&path, repos);
        }
    }
}

fn admin_repo_from_gitdir(gitdir: &Path) -> Option<PathBuf> {
    // gitdir is like <repo>/.git/worktrees/<name>
    // go up 3 levels to get <repo>
    let worktrees_dir = gitdir.parent()?;
    if worktrees_dir.file_name()?.to_str()? != "worktrees" {
        return None;
    }
    let dot_git_dir = worktrees_dir.parent()?;
    if dot_git_dir.file_name()?.to_str()? != ".git" {
        return None;
    }
    let repo = dot_git_dir.parent()?;
    Some(repo.to_path_buf())
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
        let label = dir.strip_prefix(wt_root).unwrap_or(dir);
        eprintln!("wt: removed empty directory {}", label.display());
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
}

fn prune_merged(
    git: &Git,
    dry_run: bool,
    gone: bool,
    cwd: &Option<PathBuf>,
    wt_root: Option<&Path>,
) -> Result<(), String> {
    let base = match git.base_ref() {
        Ok(base) => Some(base),
        Err(e) => {
            eprintln!("wt: {e}; skipping merged worktree pruning");
            None
        }
    };

    let porcelain = git.list_worktrees()?;
    let worktrees = parse_porcelain(&porcelain);

    let mut errors = 0usize;

    for wt in worktrees.iter().skip(1) {
        let Some(branch) = &wt.branch else {
            continue;
        };
        if wt.locked {
            continue;
        }

        let branch_ref = format!("refs/heads/{branch}");
        let ancestor = base
            .as_ref()
            .is_some_and(|base_ref| git.is_ancestor(&branch_ref, base_ref));
        let upstream_gone = gone && git.is_upstream_gone(branch);

        if !ancestor && !upstream_gone {
            continue;
        }

        let reason = if ancestor { "merged" } else { "upstream gone" };

        let path = &wt.path;
        let label = if let Some(root) = wt_root
            && let Ok(canonical) = path.canonicalize()
            && let Ok(rel) = canonical.strip_prefix(root)
        {
            rel.display().to_string()
        } else {
            branch.to_string()
        };

        if let Some(cwd) = cwd
            && let Ok(canonical) = path.canonicalize()
            && (cwd == &canonical || cwd.starts_with(&canonical))
        {
            eprintln!("wt: skipping {label} ({reason}, current directory)");
            continue;
        }

        if git.is_dirty(path) {
            continue;
        }

        if dry_run {
            eprintln!("wt: would remove {label} ({reason})");
            continue;
        }

        if let Err(e) = git.remove_worktree(path, false) {
            eprintln!("wt: {e}");
            errors += 1;
            continue;
        }

        let force_delete = upstream_gone;
        if let Err(e) = git.delete_branch(branch, force_delete) {
            eprintln!("wt: {e}");
            errors += 1;
            continue;
        }

        eprintln!("wt: removed {label} ({reason})");
    }

    if errors > 0 {
        return Err(format!("cannot clean up {errors} worktree(s)"));
    }

    Ok(())
}
