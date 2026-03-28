use std::path::Path;

use crate::commands::link;
use crate::fuzzy;
use crate::git::Git;
use crate::terminal;
use crate::worktree;

pub fn run(name: &str, create: bool, repo: Option<&Path>) -> Result<(), String> {
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
                eprintln!("pruning stale worktree metadata");
                if let Err(e) = git.prune_worktrees(false) {
                    eprintln!("{e}");
                }
            }
            println!("{}", one.path.display());
            return Ok(());
        }
        [_, _, ..] => {
            eprintln!("ambiguous name '{name}'; matches:");
            for m in &matches {
                eprintln!("  - {}", m.path.display());
            }
            return Err("multiple worktrees match, remove duplicates with `wt rm`".into());
        }
        [] => {}
    }

    if has_prunable {
        eprintln!("pruning stale worktree metadata");
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
            "branch '{name}' exists on multiple remotes: {}, use `wt new <remote>/{name}` instead",
            remotes.join(", ")
        ));
    }

    let is_branch = is_local || !remotes.is_empty();
    if !is_branch && git.rev_resolves(name) {
        return Err(format!(
            "'{name}' is not a branch, use `wt new {name}` to check out a ref"
        ));
    }

    if !is_branch && !create {
        let owned = git.local_branches();
        let branches: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        if let Some(suggestion) = fuzzy::close_match(name, &branches) {
            return Err(format!(
                "did you mean '{suggestion}'? use `wt switch -c {name}` to create a new branch"
            ));
        }
    }

    let dest = worktree::create_dest(&repo_root, &git)?;

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
        eprintln!("checking out '{name}'");
    } else {
        eprintln!("creating branch '{name}'");
    }

    let primary_path = worktree::find_primary(&worktrees, &repo_root)
        .map_or(repo_root.as_path(), |wt| wt.path.as_path());
    link::auto_link(&repo_root, &dest, primary_path);

    println!("{}", dest.display());

    terminal::print_cd_hint(name);
    Ok(())
}
