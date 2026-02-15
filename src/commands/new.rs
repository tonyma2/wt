use std::path::Path;

use crate::git::Git;

pub fn run(
    name: &str,
    create: bool,
    base: Option<&str>,
    repo: Option<&Path>,
) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

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

    if create {
        if git.has_local_branch(name) {
            return Err(format!(
                "cannot create branch '{name}': already exists; use 'wt new {name}'"
            ));
        }
        git.add_worktree(name, &dest, base)?;
        eprintln!("wt: creating branch '{name}'");
    } else {
        git.checkout_worktree(name, &dest)?;
        eprintln!("wt: checking out '{name}'");
    }

    println!("{}", dest.display());
    Ok(())
}
