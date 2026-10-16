use std::fs;
use std::path::Path;

use crate::commands::link::{self, in_linked_dir, is_expected_link};
use crate::config::{Config, normalize_path};
use crate::git::Git;
use crate::worktree;

pub fn run(files: &[String], repo: Option<&Path>, all: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let worktrees = worktree::parse_porcelain(&Git::new(&repo_root).list_worktrees()?);
    let config = Config::load()?;
    let primary =
        link::find_primary(&worktrees, &repo_root, &config).ok_or("no worktrees found")?;

    let files = if all {
        let configured = config.links(&primary.path).to_vec();
        if configured.is_empty() {
            eprintln!("no linked files in config");
            return Ok(());
        }
        configured
    } else {
        files
            .iter()
            .map(|f| normalize_path(f))
            .collect::<Result<_, _>>()?
    };

    let linked = link::linked_worktrees(&worktrees, primary);
    let configured = config.links(&primary.path);
    if let Some(file) = files.iter().find(|file| {
        !configured.contains(file)
            && !linked
                .iter()
                .any(|wt| is_expected_link(&wt.path.join(file), &primary.path.join(file)))
    }) {
        return Err(format!("cannot unlink {file}: not linked"));
    }
    if linked.is_empty() {
        eprintln!("no linked worktrees");
    }

    let mut errors = 0usize;
    let mut file_errors = vec![false; files.len()];

    for wt in &linked {
        let label = wt.label();
        for (i, file) in files.iter().enumerate() {
            let source = primary.path.join(file);
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

            if let Err(e) = fs::remove_file(&dest) {
                eprintln!("cannot remove {file} in {label}: {e}");
                errors += 1;
                file_errors[i] = true;
                continue;
            }
            remove_empty_parents(&wt.path, &dest);
            eprintln!("unlinked {file} in {label}");
        }
    }

    let succeeded: Vec<String> = files
        .iter()
        .zip(&file_errors)
        .filter(|(_, failed)| !**failed)
        .map(|(f, _)| f.clone())
        .collect();

    if !succeeded.is_empty()
        && let Err(e) = Config::update(|config| config.remove_links(&primary.path, &succeeded))
    {
        eprintln!("cannot update link config: {e}");
    }

    if errors > 0 {
        return Err(format!(
            "cannot unlink {errors} {}",
            if errors == 1 { "file" } else { "files" }
        ));
    }
    Ok(())
}

fn remove_empty_parents(wt: &Path, dest: &Path) {
    for dir in dest.ancestors().skip(1).take_while(|dir| *dir != wt) {
        if fs::remove_dir(dir).is_err() {
            break;
        }
    }
}
