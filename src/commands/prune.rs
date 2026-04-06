use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::terminal::{self, Colors};
use crate::worktree;

pub fn run(
    dry_run: bool,
    gone: bool,
    stale: bool,
    repo: Option<&Path>,
    base: Option<&str>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().and_then(|p| p.canonicalize()).ok();

    let clr = terminal::stderr_colors();

    if let Some(repo_path) = repo {
        let repo_root = Git::find_repo(Some(repo_path))?;
        let git = Git::new(&repo_root);
        let output = git.prune_worktrees(dry_run)?;
        if !output.is_empty() {
            for line in output.lines() {
                eprintln!("{}", style_msg(line, &clr));
            }
        }
        let mut msgs = Vec::new();
        let result = prune_merged(&git, dry_run, gone, stale, cwd.as_deref(), base, &mut msgs);
        for msg in &msgs {
            eprintln!("{}", style_msg(msg, &clr));
        }
        return result;
    }

    let wt_root = worktree::worktrees_root()?;

    if !wt_root.is_dir() {
        return Ok(());
    }
    let wt_root = worktree::canonicalize_or_self(&wt_root);

    let repos = worktree::discover_repos(&wt_root);
    let mut errors = 0usize;
    let mut printed = false;
    for repo_path in &repos {
        if !repo_path.exists() {
            continue;
        }
        let git = Git::new(repo_path);
        let mut repo_msgs: Vec<String> = Vec::new();

        match git.prune_worktrees(dry_run) {
            Ok(output) if !output.is_empty() => {
                for line in output.lines() {
                    repo_msgs.push(line.to_string());
                }
            }
            Err(e) => {
                eprintln!(
                    "{}cannot prune {}: {e}{}",
                    clr.red,
                    repo_path.display(),
                    clr.reset
                );
                errors += 1;
                continue;
            }
            _ => {}
        }

        if let Err(e) = prune_merged(
            &git,
            dry_run,
            gone,
            stale,
            cwd.as_deref(),
            base,
            &mut repo_msgs,
        ) {
            repo_msgs.push(format!("cannot clean up: {e}"));
            errors += 1;
        }

        if !repo_msgs.is_empty() {
            if printed {
                eprintln!();
            }
            let name = worktree::repo_basename(repo_path);
            eprintln!("{}{}:{}", clr.bold, name, clr.reset);
            for msg in &repo_msgs {
                eprintln!("  {}", style_msg(msg, &clr));
            }
            printed = true;
        }
    }

    let mut orphans = find_orphans(&wt_root);
    let mut has_orphan_output = false;
    orphans.retain(|orphan| {
        if worktree::is_cwd_inside(orphan, cwd.as_deref()) {
            let label = orphan.strip_prefix(&wt_root).unwrap_or(orphan.as_path());
            if printed && !has_orphan_output {
                eprintln!();
            }
            eprintln!(
                "{}",
                style_msg(
                    &format!("skipping {} (orphan, current directory)", label.display()),
                    &clr,
                )
            );
            has_orphan_output = true;
            false
        } else {
            true
        }
    });

    if dry_run {
        if printed && !has_orphan_output && !orphans.is_empty() {
            eprintln!();
        }
        for orphan in &orphans {
            let label = orphan.strip_prefix(&wt_root).unwrap_or(orphan.as_path());
            eprintln!(
                "{}",
                style_msg(&format!("would remove {} (orphan)", label.display()), &clr,)
            );
        }
    } else {
        if printed && !has_orphan_output && !orphans.is_empty() {
            eprintln!();
        }
        for orphan in &orphans {
            fs::remove_dir_all(orphan)
                .map_err(|e| format!("cannot remove {}: {e}", orphan.display()))?;
            let label = orphan.strip_prefix(&wt_root).unwrap_or(orphan.as_path());
            eprintln!(
                "{}",
                style_msg(&format!("removed {} (orphan)", label.display()), &clr,)
            );
        }
        cleanup_empty_parents(&orphans, &wt_root, cwd.as_deref(), &clr);
    }

    if errors > 0 {
        return Err(format!(
            "cannot prune {errors} {}",
            if errors == 1 { "repo" } else { "repos" }
        ));
    }

    Ok(())
}

fn find_orphans(wt_root: &Path) -> Vec<PathBuf> {
    let mut orphans = Vec::new();
    scan_dir(wt_root, wt_root, &mut orphans);
    orphans
}

fn scan_dir(dir: &Path, wt_root: &Path, orphans: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("cannot read directory {}: {e}", dir.display());
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("cannot read entry in {}: {e}", dir.display());
                continue;
            }
        };

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        let dot_git = path.join(".git");

        if dot_git.is_file() {
            if let Some(gitdir) = worktree::parse_gitdir(&dot_git) {
                if !gitdir.exists() {
                    orphans.push(path);
                }
            } else {
                eprintln!("cannot parse {}, skipping", dot_git.display());
            }
        } else if !dot_git.is_dir() {
            if dir.parent() == Some(wt_root) {
                // sole empty dir at the <id> level → zombie from interrupted create_dest
                if fs::read_dir(&path).is_ok_and(|mut d| d.next().is_none())
                    && fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some() && d.next().is_none())
                {
                    orphans.push(path);
                }
            } else {
                scan_dir(&path, wt_root, orphans);
            }
        }
    }
}

fn cleanup_empty_parents(orphans: &[PathBuf], wt_root: &Path, cwd: Option<&Path>, clr: &Colors) {
    let candidates: BTreeSet<&Path> = orphans.iter().filter_map(|p| p.parent()).collect();
    let mut sorted: Vec<&Path> = candidates.into_iter().collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

    for dir in sorted {
        cleanup_dir_chain(dir, wt_root, cwd, clr);
    }
}

fn cleanup_dir_chain(mut dir: &Path, wt_root: &Path, cwd: Option<&Path>, clr: &Colors) {
    while dir != wt_root && dir.starts_with(wt_root) {
        let is_empty = fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none());
        if !is_empty {
            break;
        }
        if worktree::is_cwd_inside(dir, cwd) {
            break;
        }
        if fs::remove_dir(dir).is_err() {
            break;
        }
        let label = dir.strip_prefix(wt_root).unwrap_or(dir);
        eprintln!(
            "{}removed empty directory {}{}",
            clr.dim,
            label.display(),
            clr.reset
        );
        let Some(p) = dir.parent() else { break };
        dir = p;
    }
}

fn prune_merged(
    git: &Git,
    dry_run: bool,
    gone: bool,
    stale: bool,
    cwd: Option<&Path>,
    base_override: Option<&str>,
    messages: &mut Vec<String>,
) -> Result<(), String> {
    struct PruneCandidate {
        branch: String,
        path: PathBuf,
        merged: bool,
        remote: Option<String>,
        no_upstream: bool,
    }

    let base = if let Some(b) = base_override {
        if !git.rev_resolves(b) {
            messages.push(format!(
                "base branch '{b}' not found, skipping merged worktree pruning"
            ));
            None
        } else {
            Some(b.to_string())
        }
    } else {
        match git.base_ref() {
            Ok(base) => Some(base),
            Err(e) => {
                messages.push(format!("{e}, skipping merged worktree pruning"));
                None
            }
        }
    };
    let base_branch = base_override
        .map(|b| b.strip_prefix("origin/").unwrap_or(b))
        .or_else(|| base.as_deref().and_then(|b| b.strip_prefix("origin/")));

    let output = git.list_worktrees()?;
    let worktrees = worktree::parse_porcelain(&output);
    let candidates: Vec<PruneCandidate> = worktrees
        .iter()
        .skip(1)
        .filter_map(|wt| {
            let branch = wt.branch.as_ref()?;
            if wt.locked || wt.prunable || base_branch.is_some_and(|b| b == branch) {
                return None;
            }

            let branch_ref = format!("refs/heads/{branch}");
            let upstream = git.upstream_remote(branch);

            let no_upstream = upstream.is_none();

            if no_upstream && base_override.is_none() && !stale {
                let merged = base
                    .as_ref()
                    .is_some_and(|base_ref| git.is_ancestor(&branch_ref, base_ref));
                if merged {
                    messages.push(format!("skipping {branch} (no upstream)"));
                }
                return None;
            }

            let merged = base
                .as_ref()
                .is_some_and(|base_ref| git.is_ancestor(&branch_ref, base_ref));

            Some(PruneCandidate {
                branch: branch.clone(),
                path: wt.path.clone(),
                merged,
                remote: if gone { upstream } else { None },
                no_upstream: no_upstream && stale,
            })
        })
        .collect();

    for wt in worktrees.iter().skip(1) {
        if !wt.locked {
            continue;
        }
        let Some(branch) = &wt.branch else { continue };
        if base_branch.is_some_and(|b| b == branch.as_str()) {
            continue;
        }
        let branch_ref = format!("refs/heads/{branch}");
        let upstream = git.upstream_remote(branch);
        let is_merged = base
            .as_ref()
            .is_some_and(|base_ref| git.is_ancestor(&branch_ref, base_ref));

        if (base_override.is_some() || upstream.is_some()) && is_merged {
            messages.push(format!("skipping {branch} (merged, locked)"));
        } else if upstream.is_none() && stale {
            messages.push(format!("skipping {branch} (no upstream, locked)"));
        }
    }

    let mut gone_remote_status = BTreeMap::new();

    if gone && !dry_run {
        let remotes: BTreeSet<String> =
            candidates.iter().filter_map(|c| c.remote.clone()).collect();
        for remote in remotes {
            let fetched = if !git.has_remote(&remote) {
                messages.push(format!(
                    "remote '{remote}' not found, skipping upstream-gone pruning"
                ));
                false
            } else {
                messages.push(format!("fetching from '{remote}'"));
                git.fetch_remote(&remote)
                    .inspect_err(|e| messages.push(format!("{e}, skipping upstream-gone pruning")))
                    .is_ok()
            };
            gone_remote_status.insert(remote, fetched);
        }
    }

    let mut errors = 0usize;

    for candidate in candidates {
        let upstream_gone = if !gone {
            false
        } else if dry_run {
            git.is_upstream_gone(&candidate.branch)
        } else {
            candidate.remote.as_ref().is_some_and(|remote| {
                gone_remote_status.get(remote).copied().unwrap_or(false)
                    && git.is_upstream_gone(&candidate.branch)
            })
        };

        if !candidate.merged && !upstream_gone && !candidate.no_upstream {
            continue;
        }

        let reason = build_reason(candidate.merged, upstream_gone, candidate.no_upstream);

        let label = &candidate.branch;

        if worktree::is_cwd_inside(&candidate.path, cwd) {
            messages.push(format!("skipping {label} ({reason}, current directory)"));
            continue;
        }

        if git.is_dirty(&candidate.path) {
            messages.push(format!("skipping {label} ({reason}, dirty)"));
            continue;
        }

        if dry_run {
            messages.push(format!("would remove {label} ({reason})"));
            continue;
        }

        if let Err(e) = git.remove_worktree(&candidate.path, false) {
            messages.push(e);
            errors += 1;
            continue;
        }

        worktree::cleanup_empty_parent(&candidate.path, cwd);

        if let Err(e) = git.delete_branch(&candidate.branch, true) {
            messages.push(e);
            errors += 1;
            continue;
        }

        messages.push(format!("removed {label} ({reason})"));
    }

    if errors > 0 {
        return Err(format!(
            "cannot clean up {errors} {}",
            if errors == 1 { "worktree" } else { "worktrees" }
        ));
    }

    Ok(())
}

fn build_reason(merged: bool, upstream_gone: bool, no_upstream: bool) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if merged {
        parts.push("merged");
    }
    if upstream_gone {
        parts.push("upstream gone");
    }
    if no_upstream {
        parts.push("no upstream");
    }
    parts.join(", ")
}

fn style_msg(msg: &str, clr: &Colors) -> String {
    if let Some(rest) = msg.strip_prefix("removed ") {
        style_action(clr.green, "removed", rest, clr)
    } else if let Some(rest) = msg.strip_prefix("would remove ") {
        style_action(clr.yellow, "would remove", rest, clr)
    } else if let Some(rest) = msg.strip_prefix("skipping ") {
        style_action(clr.yellow, "skipping", rest, clr)
    } else if msg.starts_with("fetching ") || msg.starts_with("Removing ") {
        format!("{}{msg}{}", clr.dim, clr.reset)
    } else {
        format!("{}{msg}{}", clr.red, clr.reset)
    }
}

fn style_action(verb_clr: &str, verb: &str, rest: &str, clr: &Colors) -> String {
    if let Some(pos) = rest.rfind('(') {
        let target = &rest[..pos];
        let reason = &rest[pos..];
        format!(
            "{verb_clr}{verb}{} {target}{}{reason}{}",
            clr.reset, clr.dim, clr.reset
        )
    } else {
        format!("{verb_clr}{verb}{} {rest}", clr.reset)
    }
}
