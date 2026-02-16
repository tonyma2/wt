use std::path::{Path, PathBuf};

use crate::git::Git;

fn random_id() -> Result<String, String> {
    let mut buf = [0u8; 3];
    getrandom::fill(&mut buf).map_err(|e| format!("cannot generate random id: {e}"))?;
    Ok(format!("{:02x}{:02x}{:02x}", buf[0], buf[1], buf[2]))
}

fn unique_dest(wt_base: &Path, repo_name: &str) -> Result<PathBuf, String> {
    for _ in 0..10 {
        let id = random_id()?;
        let candidate = wt_base.join(id).join(repo_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("cannot generate unique worktree path".to_string())
}

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
    let wt_base = Path::new(&home).join(".wt").join("worktrees");
    let dest = unique_dest(&wt_base, repo_name)?;
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("cannot create directory {}: {e}", dest.display()))?;

    let result = if create {
        if git.has_local_branch(name) {
            Err(format!(
                "cannot create branch '{name}': already exists; use 'wt new {name}'"
            ))
        } else {
            git.add_worktree(name, &dest, base)
        }
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
