use std::path::Path;

use crate::commands::link::{is_expected_link, remove_dest, validate_path};
use crate::config;
use crate::git::Git;
use crate::worktree;

pub fn run(files: &[String], repo: Option<&Path>, force: bool, all: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;

    let files = if all {
        let cfg = config::load()?;
        let linked = cfg
            .links
            .get(&config::repo_key(&repo_root))
            .cloned()
            .unwrap_or_default();
        if linked.is_empty() {
            eprintln!("no linked files in config");
            return Ok(());
        }
        linked
    } else {
        files.to_vec()
    };

    let git = Git::new(&repo_root);
    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    for file in &files {
        validate_path(file)?;
    }

    let canonical_root = worktree::canonicalize_or_self(&repo_root);
    let primary = worktrees
        .iter()
        .find(|wt| worktree::canonicalize_or_self(&wt.path) == canonical_root)
        .or_else(|| worktrees.iter().find(|wt| !wt.bare))
        .ok_or("no worktrees found")?;
    let primary_path = &primary.path;

    let linked: Vec<_> = worktrees
        .iter()
        .filter(|wt| !wt.bare && wt.path != primary.path)
        .collect();
    if linked.is_empty() {
        eprintln!("no linked worktrees");
        return Ok(());
    }

    let mut errors = 0usize;
    let mut file_errors = vec![false; files.len()];

    for wt in &linked {
        for (i, file) in files.iter().enumerate() {
            let source = primary_path.join(file);
            let dest = wt.path.join(file);

            let Ok(meta) = dest.symlink_metadata() else {
                continue;
            };

            let is_correct_link = is_expected_link(&dest, &source);

            if !is_correct_link && !force {
                if meta.file_type().is_symlink() {
                    eprintln!(
                        "skipped {file} ({}): symlink points elsewhere",
                        wt.path.display()
                    );
                } else {
                    eprintln!("skipped {file} ({}): not a symlink", wt.path.display());
                }
                continue;
            }

            if let Err(e) = remove_dest(&dest) {
                eprintln!("cannot remove {} in {}: {e}", file, wt.path.display());
                errors += 1;
                file_errors[i] = true;
                continue;
            }
            eprintln!("unlinked {file} ({})", wt.path.display());
        }
    }

    let succeeded: Vec<String> = files
        .iter()
        .zip(&file_errors)
        .filter(|(_, had_error)| !**had_error)
        .map(|(f, _)| f.clone())
        .collect();

    if !succeeded.is_empty()
        && let Err(e) = config::remove_links(&repo_root, &succeeded)
    {
        eprintln!("cannot update link config: {e}");
    }

    if errors > 0 {
        Err(format!(
            "cannot unlink {errors} {}",
            if errors == 1 { "file" } else { "files" }
        ))
    } else {
        Ok(())
    }
}
