use std::path::Path;

use crate::git::Git;

pub fn run(name: &str, base: Option<&str>, repo: Option<&Path>) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);

    if let Some((prefix, rest)) = name.split_once('/')
        && git.list_remotes().iter().any(|r| r == prefix)
    {
        let suggestion = match base {
            Some(b) => format!("wt new {rest} --base {b}"),
            None => format!("wt new {rest}"),
        };
        return Err(format!(
            "'{name}' is a remote ref; use '{suggestion}' instead"
        ));
    }

    let local_exists = git.has_local_branch(name);

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

    if let Some(base) = base {
        if local_exists {
            return Err(format!("cannot use --base: branch '{name}' already exists"));
        }
        git.add_worktree(name, &dest, Some(base))?;
    } else if local_exists {
        eprintln!("wt: branch '{name}' exists, checking out");
        git.checkout_worktree(name, &dest)?;
    } else {
        match git.checkout_worktree(name, &dest) {
            Ok(()) => eprintln!("wt: checking out '{name}'"),
            Err(_) => {
                git.add_worktree(name, &dest, None)?;
                eprintln!("wt: creating branch '{name}'");
            }
        }
    }

    println!("{}", dest.display());
    Ok(())
}
