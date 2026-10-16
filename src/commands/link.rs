use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::Path;

use crate::config::{Config, normalize_path};
use crate::git::Git;
use crate::layout;
use crate::terminal;
use crate::worktree::{self, Worktree};

pub fn run(files: &[String], repo: Option<&Path>, force: bool, list: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let worktrees = worktree::parse_porcelain(&Git::new(&repo_root).list_worktrees()?);
    let config = Config::load()?;
    let primary = find_primary(&worktrees, &repo_root, &config).ok_or("no worktrees found")?;

    if list {
        let links = config.links(&primary.path);
        if links.is_empty() {
            eprintln!("no links configured");
        }
        for link in links {
            println!("{link}");
        }
        return Ok(());
    }

    let files = files
        .iter()
        .map(|f| normalize_path(f))
        .collect::<Result<Vec<_>, _>>()?;
    for file in &files {
        if !primary.path.join(file).exists() {
            return Err(format!(
                "cannot link {file}: not found in primary worktree ({})",
                terminal::tilde_path(&primary.path)
            ));
        }
    }

    if let Err(e) = Config::update(|config| config.add_links(&primary.path, &files)) {
        eprintln!("cannot save link config: {e}");
    }

    let linked = linked_worktrees(&worktrees, primary);
    if linked.is_empty() {
        eprintln!("no linked worktrees");
        return Ok(());
    }

    let mut errors = 0usize;
    for wt in linked {
        for file in &files {
            if let Err(e) = link_into(&primary.path, &wt.path, &wt.label(), file, force) {
                eprintln!("{e}");
                errors += 1;
            }
        }
    }

    if errors > 0 {
        return Err(format!(
            "cannot link {errors} {}",
            if errors == 1 { "file" } else { "files" }
        ));
    }
    Ok(())
}

fn link_into(
    primary: &Path,
    wt: &Path,
    label: &str,
    file: &str,
    force: bool,
) -> Result<(), String> {
    let source = primary.join(file);
    let dest = wt.join(file);

    if in_linked_dir(wt, &dest) {
        eprintln!("skipped {file} in {label}: inside a linked directory");
        return Ok(());
    }
    if dest.symlink_metadata().is_ok() {
        if is_expected_link(&dest, &source) {
            return Ok(());
        }
        if !force {
            eprintln!("skipped {file} in {label}: already exists, use --force to replace");
            return Ok(());
        }
        remove_dest(&dest).map_err(|e| format!("cannot remove {file} in {label}: {e}"))?;
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create directory {}: {e}", parent.display()))?;
    }
    symlink(&source, &dest).map_err(|e| format!("cannot link {file} in {label}: {e}"))?;
    eprintln!("linked {file} in {label}");
    Ok(())
}

pub fn auto_link(worktrees: &[Worktree], repo_root: &Path, dest: &Path, label: &str) {
    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("cannot auto-link: {e}");
            return;
        }
    };
    let Some(primary) = find_primary(worktrees, repo_root, &config) else {
        return;
    };

    for entry in config.links(&primary.path) {
        let file = match normalize_path(entry) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("cannot auto-link {entry}: {e}");
                continue;
            }
        };
        if primary.path.join(&file).exists()
            && let Err(e) = link_into(&primary.path, dest, label, &file, false)
        {
            eprintln!("{e}");
        }
    }
}

pub(crate) fn find_primary<'a>(
    worktrees: &'a [Worktree],
    repo_root: &Path,
    config: &Config,
) -> Option<&'a Worktree> {
    let current = worktree::find_by_path(worktrees, repo_root).filter(|wt| !wt.bare);
    let has_links = |wt: &&Worktree| !config.links(&wt.path).is_empty();
    current
        .filter(has_links)
        .or_else(|| worktrees.iter().filter(|wt| wt.live()).find(has_links))
        .or(current)
        .or_else(|| worktrees.iter().find(|wt| !wt.bare))
}

pub(crate) fn linked_worktrees<'a>(
    worktrees: &'a [Worktree],
    primary: &Worktree,
) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|wt| !wt.bare && wt.path != primary.path && wt.live())
        .collect()
}

pub(crate) fn in_linked_dir(wt: &Path, dest: &Path) -> bool {
    dest.parent()
        .and_then(|parent| parent.ancestors().find(|p| p.exists()))
        .is_some_and(|existing| {
            !layout::canonicalize_or_self(existing).starts_with(layout::canonicalize_or_self(wt))
        })
}

pub(crate) fn is_expected_link(dest: &Path, source: &Path) -> bool {
    fs::read_link(dest).is_ok_and(|target| target == source)
}

fn remove_dest(dest: &Path) -> io::Result<()> {
    if dest.symlink_metadata().is_ok_and(|m| m.is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination is a directory, remove it manually",
        ));
    }
    fs::remove_file(dest)
}
