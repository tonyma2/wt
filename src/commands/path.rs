use std::path::Path;

use crate::fuzzy;
use crate::git::Git;
use crate::worktree;

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;

    let git = Git::new(&repo_root);
    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);
    let matches = worktree::find_live_by_branch(&worktrees, name);

    if matches.len() == 1 {
        println!("{}", matches[0].path.display());
        return Ok(());
    }
    if matches.len() > 1 {
        eprintln!("ambiguous name '{name}'; matches:");
        for m in &matches {
            eprintln!("  - {}", m.path.display());
        }
        return Err("multiple worktrees match, specify a path instead".into());
    }

    if let Some(sha) = git.rev_parse(name) {
        let head_matches = worktree::find_live_by_head(&worktrees, &sha);
        if head_matches.len() == 1 {
            println!("{}", head_matches[0].path.display());
            return Ok(());
        }
        if head_matches.len() > 1 {
            eprintln!("ambiguous ref '{name}'; matches:");
            for m in &head_matches {
                eprintln!("  - {}", m.path.display());
            }
            return Err("multiple worktrees match, specify a path instead".into());
        }
    }

    let branches: Vec<&str> = worktrees
        .iter()
        .filter_map(|wt| wt.branch.as_deref())
        .collect();
    if let Some(suggestion) = fuzzy::close_match(name, &branches) {
        return Err(format!(
            "no worktree found for: {name}, did you mean '{suggestion}'?"
        ));
    }
    Err(format!("no worktree found for: {name}"))
}
