use std::fs::{self, File};
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};

use crate::git::Git;
use crate::layout;
use crate::terminal;
use crate::worktree::{self, Worktree};

struct Store {
    dir: PathBuf,
}

impl Store {
    fn of(git: &Git) -> Result<Self, String> {
        let common_dir = git
            .common_dir()
            .ok_or("cannot locate the repository's git directory")?;
        Ok(Self {
            dir: layout::canonicalize_or_self(&common_dir).join("wt/links"),
        })
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    fn files(&self) -> Result<Vec<String>, String> {
        let mut paths = Vec::new();
        collect_files(&self.dir, &mut paths)
            .map_err(|e| format!("cannot read {}: {e}", self.dir.display()))?;
        let mut files: Vec<String> = paths
            .iter()
            .filter_map(|p| p.strip_prefix(&self.dir).ok())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        files.sort();
        Ok(files)
    }

    fn add(&self, file: &str, src: &Path) -> Result<(), String> {
        let dest = self.path(file);
        dest.parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| move_file(src, &dest))
            .map_err(|e| match e.kind() {
                io::ErrorKind::AlreadyExists => format!(
                    "cannot link {file}: {} already exists",
                    terminal::tilde_path(&dest)
                ),
                _ => format!(
                    "cannot move {file} into {}: {e}",
                    terminal::tilde_path(&dest)
                ),
            })?;
        symlink(&dest, src).map_err(|e| {
            format!(
                "moved {file} into {} but cannot link it back: {e}",
                terminal::tilde_path(&dest)
            )
        })
    }
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        entries => entries?,
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            collect_files(&entry.path(), files)?;
        } else {
            files.push(entry.path());
        }
    }
    Ok(())
}

// Unlike rename, hard_link and create_new fail if the entry exists (e.g. `.Env` vs `.env`).
fn move_file(src: &Path, dest: &Path) -> io::Result<()> {
    match fs::hard_link(src, dest) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => {
            File::create_new(dest)?;
            fs::copy(src, dest).inspect_err(|_| {
                let _ = fs::remove_file(dest);
            })?;
        }
        result => result?,
    }
    fs::remove_file(src)
}

pub fn run(files: &[String], force: bool) -> Result<(), String> {
    let repo_root = Git::find_repo(None)?;
    let git = Git::new(&repo_root);
    let store = Store::of(&git)?;
    let files = normalize_paths(files)?;
    let shared = store.files()?;
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);
    let current = worktree::find_by_path(&worktrees, &repo_root).filter(|wt| !wt.bare);

    let mut to_store = Vec::new();
    for file in &files {
        if shared.contains(file) {
            continue;
        }
        let Some(current) = current else {
            return Err(format!(
                "cannot link {file}: not shared yet, run inside the worktree that has it"
            ));
        };
        let src = current.path.join(file);
        let meta = src
            .symlink_metadata()
            .map_err(|_| not_found_error(file, &current.path))?;
        if meta.is_symlink() {
            return Err(format!("cannot link {file}: is a symlink"));
        }
        if meta.is_dir() {
            return Err(format!(
                "cannot link {file}: is a directory, link the files inside it"
            ));
        }
        if Git::tracks(&current.path, file)? {
            return Err(format!(
                "cannot link {file}: tracked by git, only untracked files can be shared"
            ));
        }
        if !Git::is_ignored(&current.path, file) {
            eprintln!(
                "{file} is not ignored by git, it will show as untracked in every worktree \
                 and block `wt rm`, add it to .gitignore"
            );
        }
        to_store.push((file, src));
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
        for file in &files {
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
    let mut replaced = false;
    if dest.symlink_metadata().is_ok() {
        if fs::read_link(&dest).is_ok_and(|target| target == source)
            || dest.canonicalize().is_ok_and(|real| real == source)
        {
            return Ok(());
        }
        let dangling = !dest.exists();
        if !force && !dangling {
            eprintln!("skipped {file} in {label}: already exists, use --force to replace");
            return Ok(());
        }
        remove_dest(&dest).map_err(|e| format!("cannot remove {file} in {label}: {e}"))?;
        if !dangling {
            replaced = true;
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create directory {}: {e}", parent.display()))?;
    }
    symlink(source, &dest).map_err(|e| format!("cannot link {file} in {label}: {e}"))?;
    if replaced {
        eprintln!("replaced {file} in {label} with a link");
    } else {
        eprintln!("linked {file} in {label}");
    }
    Ok(())
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

fn not_found_error(file: &str, root: &Path) -> String {
    let root_display = terminal::tilde_path(root);
    let from_cwd = layout::current_dir()
        .and_then(|cwd| {
            cwd.strip_prefix(layout::canonicalize_or_self(root))
                .ok()
                .map(|rel| rel.join(file))
        })
        .filter(|rel| rel != Path::new(file) && root.join(rel).symlink_metadata().is_ok());
    match from_cwd {
        Some(rel) => format!(
            "cannot link {file}: not found in {root_display}, paths are relative to the \
             worktree root, did you mean {}?",
            rel.display()
        ),
        None => format!("cannot link {file}: not found in {root_display}"),
    }
}

fn normalize_paths(files: &[String]) -> Result<Vec<String>, String> {
    let mut unique = Vec::new();
    for file in files {
        let file = normalize_path(file)?;
        if !unique.contains(&file) {
            unique.push(file);
        }
    }
    Ok(unique)
}

fn normalize_path(file: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in Path::new(file).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy()),
            Component::CurDir => {}
            Component::ParentDir => return Err(format!("path must not contain '..': {file}")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("path must be relative: {file}"));
            }
        }
    }
    match parts.first().map(|p| p.as_ref()) {
        None => Err(format!("path must name a file: '{file}'")),
        Some(".git") => Err(format!("cannot link git metadata: {file}")),
        Some(_) => Ok(parts.join("/")),
    }
}

pub fn auto_link(git: &Git, wt: &Worktree) {
    let linked = Store::of(git).and_then(|store| {
        for file in store.files()? {
            if let Err(e) = link_into(&store.path(&file), wt, &file, false) {
                eprintln!("{e}");
            }
        }
        Ok(())
    });
    if let Err(e) = linked {
        eprintln!("cannot auto-link: {e}");
    }
}
