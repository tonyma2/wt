use std::path::Path;

use crate::config;
use crate::git::Git;
use crate::worktree;

pub fn run(files: &[String], repo: Option<&Path>, force: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);
    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    let primary = worktrees.first().ok_or("no worktrees found")?;
    let primary_path = &primary.path;

    for file in files {
        validate_path(file)?;
        let source = primary_path.join(file);
        if !source.exists() {
            return Err(format!("not found in primary worktree: {file}"));
        }
    }

    if let Err(e) = config::add_links(&repo_root, files) {
        eprintln!("cannot save link config: {e}");
    }

    let linked: Vec<_> = worktrees.iter().skip(1).collect();
    if linked.is_empty() {
        eprintln!("no linked worktrees");
        return Ok(());
    }

    for wt in &linked {
        for file in files {
            let source = primary_path.join(file);
            let dest = wt.path.join(file);

            if dest.symlink_metadata().is_ok() {
                if is_expected_link(&dest, &source) {
                    continue;
                }
                if !force {
                    eprintln!("skipped {file} ({}): already exists", wt.path.display());
                    continue;
                }
                remove_dest(&dest)
                    .map_err(|e| format!("cannot remove {} in {}: {e}", file, wt.path.display()))?;
            }

            if let Some(parent) = dest.parent()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create directory {}: {e}", parent.display()))?;
            }

            symlink(&source, &dest)
                .map_err(|e| format!("cannot link {} in {}: {e}", file, wt.path.display()))?;
            eprintln!("linked {file} ({})", wt.path.display());
        }
    }

    Ok(())
}

pub fn auto_link(repo_root: &std::path::Path, worktree_path: &std::path::Path) {
    let files = config::get_links(repo_root);
    if files.is_empty() {
        return;
    }

    let git = Git::new(repo_root);
    let output = match git.list_worktrees() {
        Ok(o) => o,
        Err(_) => return,
    };
    let worktrees = worktree::parse_porcelain(&output);
    let primary = match worktrees.first() {
        Some(p) => p,
        None => return,
    };
    let primary_path = &primary.path;

    for file in &files {
        let source = primary_path.join(file);
        if !source.exists() {
            continue;
        }
        let dest = worktree_path.join(file);
        if dest.symlink_metadata().is_ok() {
            continue;
        }
        if let Some(parent) = dest.parent()
            && !parent.exists()
        {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = symlink(&source, &dest) {
            eprintln!("cannot auto-link {file}: {e}");
        } else {
            eprintln!("auto-linked {file}");
        }
    }
}

pub(crate) fn validate_path(file: &str) -> Result<(), String> {
    let path = Path::new(file);

    if path.is_absolute() {
        return Err(format!("path must be relative: {file}"));
    }

    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(format!("path must not contain '..': {file}"));
    }

    Ok(())
}

fn is_expected_link(dest: &Path, source: &Path) -> bool {
    std::fs::read_link(dest).is_ok_and(|target| target == *source)
}

fn remove_dest(dest: &Path) -> Result<(), std::io::Error> {
    if dest
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_dir())
    {
        std::fs::remove_dir_all(dest)
    } else {
        std::fs::remove_file(dest)
    }
}

fn symlink(source: &Path, dest: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, dest)
    }
    #[cfg(not(unix))]
    {
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(source, dest)
        } else {
            std::os::windows::fs::symlink_file(source, dest)
        }
    }
}
