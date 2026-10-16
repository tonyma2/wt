use std::path::Path;

use crate::git::Git;
use crate::worktree;

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);

    let wt = worktree::resolve_worktree(&worktrees, name)?
        .ok_or_else(|| worktree::not_found_error(name, &worktrees))?;
    println!("{}", wt.path.display());
    Ok(())
}
