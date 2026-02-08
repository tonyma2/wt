use std::path::Path;

use crate::git::{self, Git};

pub fn run(name: &str, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    let exists = if git.has_local_branch(name) {
        true
    } else if git.has_origin() {
        if let Err(e) = git.fetch_origin() {
            eprintln!("wt: warning: {e}; remote branch state may be stale");
        }
        git.has_remote_branch(name)
    } else {
        false
    };

    if !exists && !git::check_ref_format(name) {
        return Err(format!("invalid branch name: {name}"));
    }

    let repo_name = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");

    let home = std::env::var("HOME").map_err(|_| "$HOME is not set".to_string())?;
    let wt_root = Path::new(&home).join(".worktrees").join(repo_name);
    std::fs::create_dir_all(&wt_root)
        .map_err(|e| format!("cannot create directory {}: {e}", wt_root.display()))?;

    let dest = wt_root.join(name);
    if dest.exists() {
        return Err(format!("path already exists: {}", dest.display()));
    }

    if exists {
        eprintln!("wt: branch '{name}' exists, checking out");
        git.checkout_worktree(name, &dest)?;
    } else {
        let base = git.base_ref().unwrap_or_else(|_| "HEAD".into());
        git.add_worktree(name, &dest, &base)?;
    }

    println!("{}", dest.display());
    Ok(())
}
