use std::path::Path;

use crate::git::Git;
use crate::layout;
use crate::terminal;

pub fn run(url: &str) -> Result<(), String> {
    let repo_name = layout::parse_repo_name(url)
        .ok_or_else(|| format!("cannot determine repo name from: {url}"))?;

    let bare_dest = layout::create_bare_dest(repo_name)?;
    clone_into(&bare_dest, url, repo_name).inspect_err(|_| layout::cleanup_dest(&bare_dest))
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
