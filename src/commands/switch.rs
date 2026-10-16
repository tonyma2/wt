use std::path::Path;

use crate::commands::link;
use crate::fuzzy;
use crate::git::{self, Git};
use crate::layout;
use crate::terminal;
use crate::worktree::{self, Worktree};

enum Start<'a> {
    Existing,
    NewBranch(Option<&'a str>),
}

pub fn run(
    name: &str,
    base: Option<&str>,
    create: bool,
    repo: Option<&Path>,
) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);

    if base.is_some() {
        if git.has_local_branch(name) {
            return Err(format!(
                "cannot create branch '{name}': already exists, use `wt switch {name}` to go there"
            ));
        }
        return create_worktree(&git, &repo_root, &worktrees, name, Start::NewBranch(base));
    }

    let live: Vec<_> = worktrees
        .iter()
        .filter(|wt| wt.branch.as_deref() == Some(name) && wt.live())
        .collect();

    match live.as_slice() {
        [one] => {
            println!("{}", one.path.display());
            terminal::print_cd_hint(&one.path);
            return Ok(());
        }
        [_, _, ..] => {
            return Err(worktree::ambiguous_error(
                name,
                &live,
                "remove duplicates with `wt rm`",
            ));
        }
        [] => {}
    }

    let is_local = git.has_local_branch(name);
    let remotes = if is_local {
        vec![]
    } else {
        git.remotes_with_branch(name)?
    };
    if remotes.len() > 1 {
        return Err(format!(
            "branch '{name}' exists on multiple remotes: {}, use `wt switch {name} <remote>/{name}` to pick one",
            remotes.join(", ")
        ));
    }

    let is_branch = is_local || !remotes.is_empty();
    if !is_branch && git.rev_parse(name).is_some() {
        return Err(format!(
            "'{name}' is not a branch, use `wt switch <branch> {name}` to start a branch from it"
        ));
    }

    if !is_branch
        && !create
        && let Some(suggestion) = similar_branch(&git, name)
    {
        return Err(format!(
            "did you mean '{suggestion}'? use `wt switch -c {name}` to create a new branch"
        ));
    }

    let remote_ref = remotes.first().map(|r| format!("refs/remotes/{r}/{name}"));
    let start = if is_local {
        Start::Existing
    } else {
        Start::NewBranch(remote_ref.as_deref())
    };
    create_worktree(&git, &repo_root, &worktrees, name, start)
}

fn similar_branch(git: &Git, name: &str) -> Option<String> {
    let branches = git.branch_names();
    let candidates: Vec<&str> = branches.iter().map(String::as_str).collect();
    fuzzy::close_match(name, &candidates).map(str::to_owned)
}

fn create_worktree(
    git: &Git,
    repo_root: &Path,
    worktrees: &[Worktree],
    name: &str,
    start: Start,
) -> Result<(), String> {
    if worktrees
        .iter()
        .any(|wt| wt.prunable && wt.branch.as_deref() == Some(name))
    {
        eprintln!("pruning stale worktree metadata");
        git.prune_worktrees(false)?;
    }

    let dest = layout::create_dest(repo_root, git)?;
    let result = match start {
        Start::Existing => git
            .checkout_worktree(name, &dest)
            .map(|()| format!("checking out '{name}'")),
        Start::NewBranch(base) => {
            let upstream = base
                .and_then(|b| git.full_ref(b))
                .filter(|r| r.starts_with("refs/remotes/") && git::branch_name(r) == Some(name));
            git.add_worktree(name, &dest, base, upstream.is_some())
                .map(|()| match upstream {
                    Some(r) => format!(
                        "creating branch '{name}' tracking '{}'",
                        r.trim_start_matches("refs/remotes/")
                    ),
                    None => format!("creating branch '{name}'"),
                })
        }
    };
    let message = result.inspect_err(|_| layout::cleanup_dest(&dest))?;

    eprintln!("{message}");
    link::auto_link(
        git,
        &Worktree {
            path: dest.clone(),
            branch: Some(name.to_owned()),
            ..Worktree::default()
        },
    );
    println!("{}", dest.display());
    terminal::print_cd_hint(&dest);
    Ok(())
}
