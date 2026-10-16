use std::fs;
use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::layout;
use crate::terminal;
use crate::worktree;

pub fn run(url: &str) -> Result<(), String> {
    let repo_name = layout::parse_repo_name(url)
        .ok_or_else(|| format!("cannot determine repo name from: {url}"))?;
    // git resolves a relative path against the bare clone, not the current directory
    let url = match Path::new(url).canonicalize() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(_) => url.to_owned(),
    };

    if let Some(existing) = existing_clone(&url) {
        return reuse_clone(&existing, &url);
    }

    let bare_dest = layout::create_bare_dest(repo_name)?;
    clone_into(&bare_dest, &url, repo_name).inspect_err(|_| layout::cleanup_dest(&bare_dest))
}

fn existing_clone(url: &str) -> Option<PathBuf> {
    let ids = fs::read_dir(layout::repos_root().ok()?).ok()?;
    ids.flatten()
        .filter_map(|id| fs::read_dir(id.path()).ok())
        .flat_map(|repos| repos.flatten().map(|repo| repo.path()))
        .find(|repo| Git::new(repo).remote_url("origin").as_deref() == Some(url))
}

fn reuse_clone(bare: &Path, url: &str) -> Result<(), String> {
    let git = Git::new(bare);
    let head = git.head_ref();
    let default_branch = head.as_deref().and_then(|h| h.strip_prefix("refs/heads/"));
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);
    let Some(wt) = worktrees
        .iter()
        .find(|wt| wt.live() && default_branch.is_some() && wt.branch.as_deref() == default_branch)
    else {
        let bare = terminal::tilde_path(bare);
        return Err(format!(
            "{url} is already cloned into {bare}, use `wt switch <branch> --repo {bare}`"
        ));
    };
    eprintln!(
        "already cloned, switching to '{}'",
        default_branch.unwrap_or_default()
    );
    println!("{}", wt.path.display());
    terminal::print_cd_hint(&wt.path);
    Ok(())
}

fn clone_into(bare_dest: &Path, url: &str, repo_name: &str) -> Result<(), String> {
    eprintln!("cloning {url}");
    Git::init_bare(bare_dest)?;
    let git = Git::new(bare_dest);
    git.add_remote("origin", url)?;
    git.fetch_remote("origin")
        .map_err(|_| format!("cannot clone {url}"))?;

    if !git.has_refs("refs/remotes/origin/") {
        return Err(format!("cannot clone {url}: repository is empty"));
    }
    // best-effort: base_ref() has fallbacks if this fails
    let _ = git.set_remote_head("origin");
    let base = git.base_ref()?;
    let default_branch = base.strip_prefix("origin/").unwrap_or(&base);
    git.track_branch(default_branch, &base)?;
    git.set_head(default_branch)?;

    let wt_dest = layout::create_worktree_dest(repo_name)?;
    git.checkout_worktree(default_branch, &wt_dest)
        .inspect_err(|_| layout::cleanup_dest(&wt_dest))?;

    eprintln!("checked out '{default_branch}'");
    println!("{}", wt_dest.display());
    terminal::print_cd_hint(&wt_dest);
    Ok(())
}
