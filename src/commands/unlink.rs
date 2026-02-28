use std::path::Path;

use crate::commands::link::validate_path;
use crate::config;
use crate::git::Git;
use crate::worktree;

pub fn run(files: &[String], repo: Option<&Path>, force: bool, all: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;

    let files = if all {
        let linked = config::get_links(&repo_root);
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

    let primary = worktrees.first().ok_or("no worktrees found")?;
    let primary_path = &primary.path;

    let linked: Vec<_> = worktrees.iter().skip(1).collect();
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

            let is_correct_link = meta.file_type().is_symlink()
                && std::fs::read_link(&dest).is_ok_and(|t| t == source);

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

            let result = if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
                std::fs::remove_dir_all(&dest)
            } else {
                std::fs::remove_file(&dest)
            };

            if let Err(e) = result {
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
        Err(format!("cannot unlink {errors} file(s)"))
    } else {
        Ok(())
    }
}
