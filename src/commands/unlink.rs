use std::fs;
use std::path::Path;

use crate::commands::link::{in_linked_dir, is_expected_link};
use crate::git::Git;
use crate::links::{Store, copy_tree, normalize_paths};
use crate::terminal;
use crate::worktree;

pub fn run(files: &[String], repo: Option<&Path>, all: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);
    let store = Store::of(&git)?;
    let _lock = store.lock()?;
    let shared = store.list()?;

    let files = if all {
        if shared.is_empty() {
            eprintln!("no linked files");
            return Ok(());
        }
        shared.clone()
    } else {
        normalize_paths(files)?
    };
    if let Some(file) = files.iter().find(|file| !shared.contains(file)) {
        return Err(format!("cannot unlink {file}: not linked"));
    }

    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);
    let live: Vec<_> = worktrees
        .iter()
        .filter(|wt| !wt.bare && wt.live())
        .collect();

    let mut errors = 0usize;
    for file in &files {
        let source = store.path(file);
        if source.symlink_metadata().is_err() {
            for wt in &live {
                let dest = wt.path.join(file);
                if is_expected_link(&dest, &source) {
                    let _ = fs::remove_file(&dest);
                }
            }
            store.remove(file)?;
            eprintln!(
                "unlinked {file}: it was missing from {}",
                terminal::tilde_path(&source)
            );
            continue;
        }

        let mut restored = 0usize;
        let mut failed = false;
        for wt in &live {
            let label = wt.label();
            let dest = wt.path.join(file);
            let Ok(meta) = dest.symlink_metadata() else {
                continue;
            };
            if in_linked_dir(&wt.path, &dest) {
                eprintln!("skipped {file} in {label}: inside a linked directory");
                continue;
            }
            if !is_expected_link(&dest, &source) {
                let reason = if meta.is_symlink() {
                    "symlink points elsewhere"
                } else {
                    "not a symlink"
                };
                eprintln!("skipped {file} in {label}: {reason}");
                continue;
            }
            match fs::remove_file(&dest).and_then(|()| copy_tree(&source, &dest)) {
                Ok(()) => {
                    eprintln!("unlinked {file} in {label}, kept a copy");
                    restored += 1;
                }
                Err(e) => {
                    eprintln!("cannot restore {file} in {label}: {e}");
                    failed = true;
                }
            }
        }

        if failed {
            errors += 1;
        } else if restored == 0 {
            eprintln!(
                "cannot unlink {file}: no worktree links to it, the only copy is {}",
                terminal::tilde_path(&source)
            );
            errors += 1;
        } else {
            store.remove(file)?;
        }
    }

    if errors > 0 {
        return Err(format!(
            "cannot unlink {errors} {}",
            if errors == 1 { "file" } else { "files" }
        ));
    }
    Ok(())
}
