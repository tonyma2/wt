use std::path::Path;

use crate::git::Git;
use crate::worktree;

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)
        .map_err(|_| "not a git repository; use --repo or run inside one".to_string())?;

    let git = Git::new(&repo_root);
    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);
    let matches = worktree::find_by_branch(&worktrees, name);

    if matches.is_empty() {
        return Err(format!("no worktree found for branch: {name}"));
    }
    if matches.len() > 1 {
        eprintln!("wt: ambiguous name '{name}'; matches:");
        for m in &matches {
            eprintln!("  - {}", m.path.display());
        }
        return Err("multiple worktrees match; specify the full branch name".into());
    }

    println!("{}", matches[0].path.display());
    Ok(())
}
