use std::path::Path;

use crate::commands::new::unique_dest;
use crate::git::Git;
use crate::worktree;

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);
    let matches = worktree::find_by_branch(&worktrees, name);

    if matches.len() > 1 {
        eprintln!("wt: ambiguous name '{name}'; matches:");
        for m in &matches {
            eprintln!("  - {}", m.path.display());
        }
        return Err("multiple worktrees match; specify the full branch name".into());
    }

    if matches.len() == 1 {
        println!("{}", matches[0].path.display());
        return Ok(());
    }

    let repo_name = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");

    let home = std::env::var("HOME").map_err(|_| "$HOME is not set".to_string())?;
    let wt_base = Path::new(&home).join(".wt").join("worktrees");
    let dest = unique_dest(&wt_base, repo_name)?;
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("cannot create directory {}: {e}", dest.display()))?;

    let create =
        !git.has_local_branch(name) && !git.rev_resolves(name) && !git.has_remote_branch(name);

    let result = if create {
        git.add_worktree(name, &dest, None)
    } else {
        git.checkout_worktree(name, &dest)
    };

    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&dest);
        if let Some(parent) = dest.parent() {
            let _ = std::fs::remove_dir(parent);
        }
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
