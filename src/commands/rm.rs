use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::layout;
use crate::terminal;
use crate::worktree::{self, Status, Worktree};

pub fn run(names: &[String], force: bool) -> Result<(), String> {
    if let [name] = names {
        return remove_one(name, force);
    }
    let mut errors = 0usize;
    for name in names {
        if let Err(e) = remove_one(name, force) {
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

fn remove_one(name_or_path: &str, force: bool) -> Result<(), String> {
    let message = remove(resolve_target(name_or_path)?, force, "--force")?;
    eprintln!("{message}");
    Ok(())
}

pub fn remove_unforced(path: &Path) -> Result<String, String> {
    remove(resolve_path(path)?, false, "wt rm --force")
}

fn remove(
    Target {
        path: target,
        admin,
        worktrees,
    }: Target,
    force: bool,
    force_hint: &str,
) -> Result<String, String> {
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

    if worktree::contains_other_worktree(&worktrees, wt) {
        return Err(format!(
            "cannot remove {label}: another worktree is inside it, remove that one first"
        ));
    }

    if wt.locked && !force {
        return Err(format!(
            "cannot remove {label}: worktree is locked, use {force_hint} to remove"
        ));
    }

    let branch = wt.branch.as_deref().filter(|b| git.has_local_branch(b));
    // git lists the main worktree (or the bare repo) first; its HEAD is the repository's
    let repo_head = worktrees
        .first()
        .and_then(|main| Git::new(&main.path).head_ref());
    let kept = branch.and_then(|b| {
        if worktree::branch_checked_out_elsewhere(&worktrees, b, &target) {
            Some(" (checked out in another worktree)")
        } else if repo_head == Some(format!("refs/heads/{b}")) {
            Some(" (the repository's HEAD)")
        } else {
            None
        }
    });
    let delete_branch = branch.filter(|_| kept.is_none());

    if !force {
        if Status::of(wt).dirty {
            return Err(format!(
                "cannot remove {label}: worktree has local changes, use {force_hint} to remove"
            ));
        }
        if let Some(branch) = delete_branch
            && !git.is_branch_merged(branch)
        {
            return Err(format!(
                "cannot remove {label}: branch is not fully merged, use {force_hint} to delete it"
            ));
        }
    }

    let sha = delete_branch.and_then(|b| git.short_sha(&format!("refs/heads/{b}")));
    git.remove_worktree(&target, force)?;
    layout::cleanup_empty_parent(&target, cwd.as_deref());

    let path_display = terminal::tilde_path(&target);
    Ok(if let Some(branch) = delete_branch {
        git.delete_branch(branch)?;
        let was = sha.map(|sha| format!(", was {sha}")).unwrap_or_default();
        format!("removed worktree and branch '{branch}' ({path_display}{was})")
    } else if let (Some(branch), Some(reason)) = (branch, kept) {
        format!("removed worktree ({path_display}), kept branch '{branch}'{reason}")
    } else {
        format!("removed worktree ({path_display})")
    })
}

fn resolve_target(name_or_path: &str) -> Result<Target, String> {
    let input = Path::new(name_or_path);
    let repo_root = match Git::find_repo(None) {
        Ok(root) => root,
        Err(_) if input.exists() => return resolve_path(input),
        Err(e) => return Err(e),
    };

    let git = Git::new(&repo_root);
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);

    let absolute = std::path::absolute(input).ok();
    let found = worktree::resolve_worktree(&worktrees, name_or_path)?.or_else(|| {
        worktrees.iter().find(|wt| {
            wt.prunable
                && (wt.branch.as_deref() == Some(name_or_path)
                    || absolute.as_ref() == Some(&wt.path))
        })
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

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn picker_removal_points_at_the_cli_to_force() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let feat = tmp.path().join("feat");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@t",
                "commit",
                "--allow-empty",
                "-m",
                "i",
            ],
        );
        git(
            &repo,
            &["worktree", "add", "-b", "feat", feat.to_str().unwrap()],
        );
        std::fs::write(feat.join("wip.txt"), "wip").unwrap();

        assert_eq!(
            remove_unforced(&feat),
            Err(
                "cannot remove 'feat': worktree has local changes, use wt rm --force to remove"
                    .into()
            )
        );
        assert!(feat.exists());
    }
}
