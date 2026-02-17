use std::path::Path;

use crate::git::Git;
use crate::worktree;

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    let branch_matches: Vec<_> = worktrees
        .iter()
        .filter(|wt| wt.branch.as_deref() == Some(name))
        .collect();
    let has_prunable = branch_matches.iter().any(|wt| wt.prunable);
    let matches: Vec<_> = branch_matches.into_iter().filter(|wt| wt.live()).collect();

    match matches.as_slice() {
        [one] => {
            if has_prunable {
                eprintln!("wt: pruning stale worktree metadata");
                if let Err(e) = git.prune_worktrees(false) {
                    eprintln!("wt: {e}");
                }
            }
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

    let is_local = git.has_local_branch(name);
    let remotes = if is_local {
        vec![]
    } else {
        git.remotes_with_branch(name)?
    };

    if !is_local && remotes.len() > 1 {
        return Err(format!(
            "branch '{name}' exists on multiple remotes: {}; use `wt new <remote>/{name}` instead",
            remotes.join(", ")
        ));
    }

    let is_branch = is_local || !remotes.is_empty();
    if !is_branch && git.rev_resolves(name) {
        return Err(format!(
            "'{name}' is not a branch; use `wt new {name}` to check out a ref"
        ));
    }

    let dest = worktree::create_dest(&repo_root)?;

    let result = if is_branch {
        git.checkout_worktree(name, &dest)
    } else {
        git.add_worktree(name, &dest, None)
    };

    if let Err(e) = result {
        worktree::cleanup_dest(&dest);
        return Err(e);
    }

    if is_branch {
        eprintln!("wt: checking out '{name}'");
    } else {
        eprintln!("wt: creating branch '{name}'");
    }

    println!("{}", dest.display());
    Ok(())
}
