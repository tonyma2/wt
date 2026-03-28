use crate::git::Git;
use crate::terminal;
use crate::worktree;

pub fn run(url: &str) -> Result<(), String> {
    let repo_name = worktree::parse_repo_name(url)
        .ok_or_else(|| format!("cannot determine repo name from: {url}"))?;

    let bare_dest = worktree::create_bare_dest(repo_name)?;

    match clone_into(&bare_dest, url, repo_name) {
        Ok(()) => Ok(()),
        Err(e) => {
            worktree::cleanup_dest(&bare_dest);
            Err(e)
        }
    }
}

fn clone_into(bare_dest: &std::path::Path, url: &str, repo_name: &str) -> Result<(), String> {
    eprintln!("cloning {url}");
    Git::bare_clone(url, bare_dest)?;

    let git = Git::new(bare_dest);

    git.set_config("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*")?;
    git.fetch_remote("origin")?;

    // best-effort: base_ref() has fallbacks if this fails
    let _ = git.set_remote_head("origin");

    let base = git.base_ref()?;
    let default_branch = base.strip_prefix("origin/").unwrap_or(&base);

    let wt_dest = worktree::create_worktree_dest(repo_name)?;
    if let Err(e) = git.checkout_worktree(default_branch, &wt_dest) {
        worktree::cleanup_dest(&wt_dest);
        return Err(e);
    }

    eprintln!("checked out '{default_branch}'");
    println!("{}", wt_dest.display());
    terminal::print_cd_hint(default_branch);
    Ok(())
}
