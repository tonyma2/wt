use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::worktree::{self, Worktree};

pub fn run(names: &[String], repo: Option<&Path>, force: bool) -> Result<(), String> {
    if names.len() == 1 {
        return remove_one(&names[0], repo, force);
    }
    let mut errors = 0u32;
    for name in names {
        if let Err(e) = remove_one(name, repo, force) {
            eprintln!("wt: {e}");
            errors += 1;
        }
    }
    if errors > 0 {
        Err(format!("{errors} worktree(s) could not be removed"))
    } else {
        Ok(())
    }
}

fn remove_one(name_or_path: &str, repo: Option<&Path>, force: bool) -> Result<(), String> {
    let (target, admin_repo, worktrees) = resolve_target(name_or_path, repo)?;

    let git = Git::new(&admin_repo);

    let wt = worktree::find_by_path(&worktrees, &target)
        .ok_or_else(|| format!("not a registered worktree: {}", target.display()))?;

    if let Some(main_wt) = worktrees.first() {
        let main_path =
            std::fs::canonicalize(&main_wt.path).unwrap_or_else(|_| main_wt.path.clone());
        if main_path == target {
            return Err(format!(
                "cannot remove the primary worktree: {}",
                target.display()
            ));
        }
    }

    let branch = wt.branch.as_deref().map(str::to_string);

    if let Some(ref branch) = branch {
        if !git.has_local_branch(branch) {
            return Err(format!("local branch not found: {branch}"));
        }

        if worktree::branch_checked_out_elsewhere(&worktrees, branch, &target) {
            return Err(format!(
                "branch '{branch}' is checked out in another worktree; remove that worktree first"
            ));
        }
    }

    if let Ok(cwd) = std::env::current_dir().and_then(|p| p.canonicalize())
        && (cwd == target || cwd.starts_with(&target))
    {
        return Err(format!(
            "cannot remove {}: current directory is inside the worktree",
            target.display()
        ));
    }

    if !force {
        if git.is_dirty(&target) {
            return Err("worktree has local changes; use --force to remove".into());
        }
        if let Some(ref branch) = branch
            && !git.is_branch_merged(branch)
        {
            return Err(format!(
                "branch '{branch}' has unpushed commits; use --force to remove"
            ));
        }
    }

    git.remove_worktree(&target, force)?;

    if let Some(parent) = target.parent()
        && is_managed_worktree_dir(parent)
        && std::fs::read_dir(parent).is_ok_and(|mut d| d.next().is_none())
    {
        let _ = std::fs::remove_dir(parent);
    }

    if let Some(ref branch) = branch {
        git.delete_branch(branch, force)?;
        eprintln!(
            "wt: removed worktree and branch '{}' ({})",
            branch,
            target.display()
        );
    } else {
        eprintln!("wt: removed worktree ({})", target.display());
    }
    Ok(())
}

fn is_managed_worktree_dir(dir: &Path) -> bool {
    let Ok(home) = std::env::var("HOME") else {
        return false;
    };
    let wt_base = Path::new(&home).join(".wt").join("worktrees");
    dir.starts_with(&wt_base) && dir.parent() == Some(wt_base.as_path())
}

fn resolve_target(
    name_or_path: &str,
    repo: Option<&Path>,
) -> Result<(PathBuf, PathBuf, Vec<Worktree>), String> {
    let repo_root = Git::find_repo(repo).ok();
    let has_repo = repo_root.is_some();

    if let Some(repo_root) = repo_root {
        let git = Git::new(&repo_root);
        let output = git.list_worktrees()?;
        let worktrees = worktree::parse_porcelain(&output);
        let matches = worktree::find_by_branch(&worktrees, name_or_path);

        if matches.len() == 1 {
            let target = matches[0].path.clone();
            return Ok((target, repo_root, worktrees));
        }
        if matches.len() > 1 {
            eprintln!("wt: ambiguous name '{name_or_path}'; matches:");
            for m in &matches {
                eprintln!("  - {}", m.path.display());
            }
            return Err("multiple worktrees match; specify a path instead".into());
        }

        let input = Path::new(name_or_path);
        if input.exists()
            && let Ok(target) = std::fs::canonicalize(input)
            && worktree::find_by_path(&worktrees, &target).is_some()
        {
            return Ok((target, repo_root, worktrees));
        }
    }

    let input = Path::new(name_or_path);
    if input.exists() {
        let target = resolve_path(input)?;
        let (admin, worktrees) = load_worktrees(&target)?;
        return Ok((target, admin, worktrees));
    }

    if has_repo {
        Err(format!("no worktree found for branch: {name_or_path}"))
    } else {
        Err("not a git repository; use --repo or run inside one".into())
    }
}

fn resolve_path(input: &Path) -> Result<PathBuf, String> {
    let abs = std::fs::canonicalize(input)
        .map_err(|_| format!("not a worktree root: {}", input.display()))?;

    let toplevel = Git::find_repo(Some(&abs))
        .map_err(|_| format!("not a worktree root: {}", input.display()))?;

    let toplevel_canon = std::fs::canonicalize(&toplevel).unwrap_or(toplevel);

    if abs != toplevel_canon {
        return Err(format!("not a worktree root: {}", input.display()));
    }

    Ok(abs)
}

fn load_worktrees(target: &Path) -> Result<(PathBuf, Vec<Worktree>), String> {
    let git = Git::new(target);
    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    let admin = worktrees
        .iter()
        .find(|wt| wt.path != target)
        .or(worktrees.first())
        .ok_or_else(|| format!("cannot resolve repository for: {}", target.display()))?;

    Ok((admin.path.clone(), worktrees))
}
