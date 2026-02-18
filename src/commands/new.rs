use std::path::Path;

use crate::git::Git;
use crate::worktree;

pub fn run(
    name: &str,
    create: bool,
    base: Option<&str>,
    repo: Option<&Path>,
) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);
    let dest = worktree::create_dest(&repo_root)?;

    let result = if create {
        if git.has_local_branch(name) {
            let repo_flag = repo
                .map(|r| format!(" --repo {}", r.display()))
                .unwrap_or_default();
            Err(format!(
                "cannot create branch '{name}': already exists; use 'wt new{repo_flag} {name}'"
            ))
        } else {
            git.add_worktree(name, &dest, base)
        }
    } else {
        git.checkout_worktree(name, &dest)
    };

    if let Err(e) = result {
        worktree::cleanup_dest(&dest);
        return Err(e);
    }

    if create {
        eprintln!("wt: creating branch '{name}'");
    } else {
        eprintln!("wt: checking out '{name}'");
    }

    println!("{}", dest.display());
    Ok(())
}
