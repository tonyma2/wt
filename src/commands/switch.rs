use std::path::Path;

use crate::git::Git;
use crate::worktree;

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    let matches = worktree::find_by_branch(&worktrees, name);
    match matches.as_slice() {
        [one] => {
            println!("{}", one.path.display());
            return Ok(());
        }
        [_, _, ..] => {
            eprintln!("wt: ambiguous name '{name}'; matches:");
            for m in &matches {
                eprintln!("  - {}", m.path.display());
            }
            return Err("multiple worktrees match; remove duplicates with `wt rm`".into());
        }
        [] => {}
    }

    let dest = worktree::create_dest(&repo_root)?;
    let create = !git.ref_or_branch_exists(name)?;

    let result = if create {
        git.add_worktree(name, &dest, None)
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
