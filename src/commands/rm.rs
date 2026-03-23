use std::path::{Path, PathBuf};

use crate::fuzzy;
use crate::git::Git;
use crate::terminal;
use crate::worktree::{self, Worktree};

pub fn run(
    names: &[String],
    repo: Option<&Path>,
    force: bool,
    keep_branch: bool,
) -> Result<(), String> {
    if names.len() == 1 {
        return remove_one(&names[0], repo, force, keep_branch);
    }
    let mut errors = 0usize;
    for name in names {
        if let Err(e) = remove_one(name, repo, force, keep_branch) {
            eprintln!("{e}");
            errors += 1;
        }
    }
    if errors > 0 {
        Err(format!(
            "cannot remove {errors} {}",
            if errors == 1 { "worktree" } else { "worktrees" }
        ))
    } else {
        Ok(())
    }
}

fn remove_one(
    name_or_path: &str,
    repo: Option<&Path>,
    force: bool,
    keep_branch: bool,
) -> Result<(), String> {
    let (target, admin_repo, worktrees) = resolve_target(name_or_path, repo)?;

    let git = Git::new(&admin_repo);

    let wt = worktree::find_by_path(&worktrees, &target)
        .ok_or_else(|| format!("not a registered worktree: {}", target.display()))?;

    if let Some(main_wt) = worktrees.first() {
        let main_path = worktree::canonicalize_or_self(&main_wt.path);
        if main_path == target {
            return Err(format!(
                "cannot remove the primary worktree: {}",
                target.display()
            ));
        }
    }

    let branch = wt.branch.clone();
    let branch_exists = branch.as_ref().is_some_and(|b| git.has_local_branch(b));

    if let Some(branch) = &branch
        && branch_exists
        && worktree::branch_checked_out_elsewhere(&worktrees, branch, &target)
    {
        return Err(format!(
            "branch '{branch}' is checked out in another worktree, remove that worktree first"
        ));
    }

    let cwd = std::env::current_dir().and_then(|p| p.canonicalize()).ok();

    if worktree::is_cwd_inside(&target, cwd.as_deref()) {
        return Err(format!(
            "cannot remove {}: current directory is inside the worktree",
            target.display()
        ));
    }

    if !force {
        if git.is_dirty(&target) {
            return Err("worktree has local changes, use --force to remove".into());
        }
        if let Some(branch) = &branch
            && branch_exists
            && !keep_branch
            && !git.is_branch_merged(branch)
        {
            return Err(format!(
                "branch '{branch}' is not fully merged, use --force to remove"
            ));
        }
    }

    git.remove_worktree(&target, force)?;

    worktree::cleanup_empty_parent(&target, cwd.as_deref());

    let path_display = terminal::tilde_path(&target);
    if let Some(branch) = &branch
        && branch_exists
        && !keep_branch
    {
        git.delete_branch(branch, force)?;
        eprintln!(
            "removed worktree and branch '{}' ({})",
            branch, path_display
        );
    } else {
        eprintln!("removed worktree ({})", path_display);
    }
    Ok(())
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
        let matches = worktree::find_live_by_branch(&worktrees, name_or_path);

        if matches.len() == 1 {
            let target = matches[0].path.clone();
            return Ok((target, repo_root, worktrees));
        }
        if matches.len() > 1 {
            eprintln!("ambiguous name '{name_or_path}'; matches:");
            for m in &matches {
                eprintln!("  - {}", m.path.display());
            }
            return Err("multiple worktrees match, specify a path instead".into());
        }

        if let Some(sha) = git.rev_parse(name_or_path) {
            let head_matches = worktree::find_live_by_head(&worktrees, &sha);
            if head_matches.len() == 1 {
                let target = head_matches[0].path.clone();
                return Ok((target, repo_root, worktrees));
            }
            if head_matches.len() > 1 {
                eprintln!("ambiguous ref '{name_or_path}'; matches:");
                for m in &head_matches {
                    eprintln!("  - {}", m.path.display());
                }
                return Err("multiple worktrees match, specify a path instead".into());
            }
        }

        let input = Path::new(name_or_path);
        if input.exists()
            && let Ok(target) = std::fs::canonicalize(input)
            && worktree::find_by_path(&worktrees, &target).is_some()
        {
            return Ok((target, repo_root, worktrees));
        }

        let branches: Vec<&str> = worktrees
            .iter()
            .filter_map(|wt| wt.branch.as_deref())
            .collect();
        if let Some(suggestion) = fuzzy::close_match(name_or_path, &branches) {
            return Err(format!(
                "no worktree found for: {name_or_path}, did you mean '{suggestion}'?"
            ));
        }
    }

    let input = Path::new(name_or_path);
    if input.exists() {
        let target = resolve_path(input)?;
        let (admin, worktrees) = load_worktrees(&target)?;
        return Ok((target, admin, worktrees));
    }

    if has_repo {
        Err(format!("no worktree found for: {name_or_path}"))
    } else {
        Err("not a git repository, use --repo or run inside one".into())
    }
}

fn resolve_path(input: &Path) -> Result<PathBuf, String> {
    let abs = std::fs::canonicalize(input)
        .map_err(|_| format!("not a worktree root: {}", input.display()))?;

    let toplevel = Git::find_repo(Some(&abs))
        .map_err(|_| format!("not a worktree root: {}", input.display()))?;

    let toplevel_canon = worktree::canonicalize_or_self(&toplevel);

    if abs != toplevel_canon {
        return Err(format!("not a worktree root: {}", input.display()));
    }

    Ok(abs)
}

fn load_worktrees(target: &Path) -> Result<(PathBuf, Vec<Worktree>), String> {
    let git = Git::new(target);
    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);

    let primary = worktrees
        .first()
        .ok_or_else(|| format!("cannot resolve repository for: {}", target.display()))?;

    Ok((primary.path.clone(), worktrees))
}
