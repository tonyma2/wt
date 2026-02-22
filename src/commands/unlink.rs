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

    let linked: Vec<_> = worktrees.iter().skip(1).collect();
    if linked.is_empty() {
        eprintln!("wt: no linked worktrees");
        return Ok(());
    }

    for wt in &linked {
        for file in files {
            let source = primary_path.join(file);
            let dest = wt.path.join(file);

            let meta = match dest.symlink_metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_correct_link = meta.file_type().is_symlink()
                && std::fs::read_link(&dest).is_ok_and(|t| t == source);

            if !is_correct_link && !force {
                if meta.file_type().is_symlink() {
                    eprintln!(
                        "wt: skipped {file} ({}): symlink points elsewhere",
                        wt.path.display()
                    );
                } else {
                    eprintln!("wt: skipped {file} ({}): not a symlink", wt.path.display());
                }
                continue;
            }

            let result = if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
                std::fs::remove_dir_all(&dest)
            } else {
                std::fs::remove_file(&dest)
            };

            result.map_err(|e| format!("cannot remove {} in {}: {e}", file, wt.path.display()))?;
            eprintln!("wt: unlinked {file} ({})", wt.path.display());
        }
    }

    if let Err(e) = config::remove_links(
        &repo_root,
        &files.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    ) {
        eprintln!("wt: cannot update link config: {e}");
    }

    Ok(())
}
