use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::Path;

use crate::git::Git;
use crate::links::{Store, normalize_path, normalize_paths};
use crate::terminal;
use crate::worktree::{self, Worktree};

pub fn run(files: &[String], repo: Option<&Path>, force: bool, list: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(repo)?;
    let git = Git::new(&repo_root);
    let store = Store::of(&git)?;

    if list {
        let shared = store.list()?;
        if shared.is_empty() {
            eprintln!("no links configured");
        }
        for file in &shared {
            println!("{file}");
        }
        return Ok(());
    }

    let files = normalize_paths(files)?;
    for file in &files {
        if let Some(dir) = files
            .iter()
            .find(|dir| *dir != file && Path::new(file).starts_with(dir))
        {
            return Err(format!(
                "cannot link {file}: inside {dir}, which is also being linked"
            ));
        }
    }
    let _lock = store.lock()?;
    let shared = store.list()?;
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);
    let current = worktree::find_by_path(&worktrees, &repo_root).filter(|wt| !wt.bare);

    let mut linkable = Vec::new();
    let mut to_store = Vec::new();
    for file in &files {
        if shared.contains(file) && store.path(file).symlink_metadata().is_ok() {
            linkable.push(file);
            continue;
        }
        let Some(current) = current else {
            return Err(format!(
                "cannot link {file}: not shared yet, run inside the worktree that has it"
            ));
        };
        let src = current.path.join(file);
        if in_linked_dir(&current.path, &src) {
            eprintln!("skipped {file}: inside a linked directory");
            continue;
        }
        let meta = src.symlink_metadata().map_err(|_| {
            format!(
                "cannot link {file}: not found in {}",
                terminal::tilde_path(&current.path)
            )
        })?;
        if meta.is_symlink() {
            return Err(format!("cannot link {file}: is a symlink"));
        }
        if Git::tracks(&current.path, file)? {
            return Err(format!(
                "cannot link {file}: tracked by git, only untracked files can be shared"
            ));
        }
        to_store.push((file, src));
        linkable.push(file);
    }

    for (file, src) in to_store {
        store.add(file, &src)?;
        eprintln!(
            "moved {file} into {}",
            terminal::tilde_path(&store.path(file))
        );
    }

    let mut errors = 0usize;
    for wt in worktrees.iter().filter(|wt| !wt.bare && wt.live()) {
        for file in &linkable {
            if let Err(e) = link_into(&store.path(file), wt, file, force) {
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

fn link_into(source: &Path, wt: &Worktree, file: &str, force: bool) -> Result<(), String> {
    let label = wt.label();
    let dest = wt.path.join(file);
    if in_linked_dir(&wt.path, &dest) {
        eprintln!("skipped {file} in {label}: inside a linked directory");
        return Ok(());
    }
    if dest.symlink_metadata().is_ok() {
        if is_expected_link(&dest, source) {
            return Ok(());
        }
        let dangling = !dest.exists();
        if !force && !dangling {
            eprintln!("skipped {file} in {label}: already exists, use --force to replace");
            return Ok(());
        }
        remove_dest(&dest).map_err(|e| format!("cannot remove {file} in {label}: {e}"))?;
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create directory {}: {e}", parent.display()))?;
    }
    symlink(source, &dest).map_err(|e| format!("cannot link {file} in {label}: {e}"))?;
    eprintln!("linked {file} in {label}");
    Ok(())
}

pub fn auto_link(git: &Git, wt: &Worktree) {
    let (store, files) = match Store::of(git).and_then(|store| {
        let files = store.list()?;
        Ok((store, files))
    }) {
        Ok(shared) => shared,
        Err(e) => {
            eprintln!("cannot auto-link: {e}");
            return;
        }
    };
    for entry in &files {
        let file = match normalize_path(entry) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("cannot auto-link {entry}: {e}");
                continue;
            }
        };
        let source = store.path(&file);
        if source.symlink_metadata().is_ok()
            && let Err(e) = link_into(&source, wt, &file, false)
        {
            eprintln!("{e}");
        }
    }
}

pub(crate) fn in_linked_dir(wt: &Path, dest: &Path) -> bool {
    dest.ancestors()
        .skip(1)
        .take_while(|dir| *dir != wt)
        .any(Path::is_symlink)
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
