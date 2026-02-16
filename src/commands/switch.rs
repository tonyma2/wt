use std::path::Path;

use crate::git::Git;
use crate::worktree;

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    let all_matches = worktree::find_by_branch(&worktrees, name);
    let has_prunable = all_matches.iter().any(|wt| wt.prunable);
    let matches: Vec<_> = all_matches.into_iter().filter(|wt| !wt.prunable).collect();
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

    if has_prunable {
        eprintln!("wt: pruning stale worktree metadata");
        git.prune_worktrees(false)?;
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
