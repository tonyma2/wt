use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::layout;
use crate::terminal;
use crate::worktree::{self, Status, Worktree};

pub fn run(
    names: &[String],
    repo: Option<&Path>,
    force: bool,
    keep_branch: bool,
) -> Result<(), String> {
    if let [name] = names {
        return remove_one(name, repo, force, keep_branch);
    }
    let mut errors = 0usize;
    for name in names {
        if let Err(e) = remove_one(name, repo, force, keep_branch) {
            eprintln!("{e}");
            errors += 1;
        }
    }
    if errors > 0 {
        return Err(format!(
            "cannot remove {errors} {}",
            if errors == 1 { "worktree" } else { "worktrees" }
        ));
    }
    Ok(())
}

struct Target {
    path: PathBuf,
    admin: PathBuf,
    worktrees: Vec<Worktree>,
}

fn remove_one(
    name_or_path: &str,
    repo: Option<&Path>,
    force: bool,
    keep_branch: bool,
) -> Result<(), String> {
    let Target {
        path: target,
        admin,
        worktrees,
    } = resolve_target(name_or_path, repo)?;
    let git = Git::new(&admin);

    let wt = worktree::find_by_path(&worktrees, &target)
        .ok_or_else(|| format!("not a registered worktree: {}", target.display()))?;

    let canonical_target = layout::canonicalize_or_self(&target);
    if worktrees
        .first()
        .is_some_and(|main| layout::canonicalize_or_self(&main.path) == canonical_target)
    {
        return Err(format!(
            "cannot remove the primary worktree: {}",
            terminal::tilde_path(&target)
        ));
    }

    let label = wt.label();
    let cwd = layout::current_dir();
    if layout::is_cwd_inside(&target, cwd.as_deref()) {
        return Err(format!(
            "cannot remove {label}: current directory is inside the worktree"
        ));
    }

    if wt.locked && !force {
        return Err(format!(
            "cannot remove {label}: worktree is locked, use --force to remove"
        ));
    }

    let branch = wt
        .branch
        .as_deref()
        .filter(|b| !keep_branch && git.has_local_branch(b));
    let in_use = branch.filter(|b| worktree::branch_checked_out_elsewhere(&worktrees, b, &target));
    let delete_branch = branch.filter(|_| in_use.is_none());

    if !force {
        if Status::of(wt).dirty {
            return Err(format!(
                "cannot remove {label}: worktree has local changes, use --force to remove"
            ));
        }
        if let Some(branch) = delete_branch
            && !git.is_branch_merged(branch)
        {
            return Err(format!(
                "cannot remove {label}: branch is not fully merged, \
                 use --keep-branch to keep it or --force to delete it"
            ));
        }
    }

    git.remove_worktree(&target, force)?;
    layout::cleanup_empty_parent(&target, cwd.as_deref());

    let path_display = terminal::tilde_path(&target);
    if let Some(branch) = delete_branch {
        git.delete_branch(branch, force)?;
        eprintln!("removed worktree and branch '{branch}' ({path_display})");
    } else if let Some(branch) = in_use {
        eprintln!(
            "removed worktree ({path_display}), kept branch '{branch}' (checked out in another worktree)"
        );
    } else {
        eprintln!("removed worktree ({path_display})");
    }
    Ok(())
}

fn resolve_target(name_or_path: &str, repo: Option<&Path>) -> Result<Target, String> {
    let input = Path::new(name_or_path);
    let repo_root = match Git::find_repo(repo) {
        Ok(root) => root,
        Err(_) if input.exists() => return resolve_path(input),
        Err(e) => return Err(e),
    };

    let git = Git::new(&repo_root);
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);

    let found = worktree::resolve_worktree(&worktrees, name_or_path)?.or_else(|| {
        worktrees
            .iter()
            .find(|wt| wt.prunable && wt.branch.as_deref() == Some(name_or_path))
    });
    if let Some(wt) = found {
        return Ok(Target {
            path: wt.path.clone(),
            admin: repo_root,
            worktrees,
        });
    }

    if input.exists() {
        return resolve_path(input);
    }
    Err(worktree::not_found_error(name_or_path, &worktrees))
}

fn resolve_path(input: &Path) -> Result<Target, String> {
    let not_root = || format!("not a worktree root: {}", input.display());

    let path = input.canonicalize().map_err(|_| not_root())?;
    let toplevel = Git::find_repo(Some(&path)).map_err(|_| not_root())?;
    if layout::canonicalize_or_self(&toplevel) != path {
        return Err(not_root());
    }

    let worktrees = worktree::parse_porcelain(&Git::new(&path).list_worktrees()?);
    let admin = worktrees
        .first()
        .map(|main| main.path.clone())
        .ok_or_else(|| format!("cannot resolve repository for: {}", path.display()))?;

    Ok(Target {
        path,
        admin,
        worktrees,
    })
}
