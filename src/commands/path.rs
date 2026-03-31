use std::path::Path;

use crate::fuzzy;
use crate::git::Git;
use crate::worktree::{self, Resolved};

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;

    let git = Git::new(&repo_root);
    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    match worktree::resolve_worktree(&worktrees, name, &git) {
        Resolved::Found(wt) => {
            println!("{}", wt.path.display());
            Ok(())
        }
        Resolved::Ambiguous { matches, kind } => {
            eprintln!("ambiguous {kind} '{name}'; matches:");
            for m in &matches {
                eprintln!("  - {}", m.path.display());
            }
            Err("multiple worktrees match, specify a path instead".into())
        }
        Resolved::NotFound => {
            let branches: Vec<&str> = worktrees
                .iter()
                .filter_map(|wt| wt.branch.as_deref())
                .collect();
            if let Some(suggestion) = fuzzy::close_match(name, &branches) {
                Err(format!(
                    "no worktree found for: {name}, did you mean '{suggestion}'?"
                ))
            } else {
                Err(format!("no worktree found for: {name}"))
            }
        }
    }
}
