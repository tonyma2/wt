use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};

use crate::git::Git;
use crate::layout;
use crate::terminal;
use crate::worktree::{self, Worktree};

// Shared files live in the repo's git directory so that no worktree owns them: removing any
// worktree only removes its symlinks.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn of(git: &Git) -> Result<Self, String> {
        let common_dir = git
            .common_dir()
            .ok_or("cannot locate the repository's git directory")?;
        Ok(Self::in_git_dir(&layout::canonicalize_or_self(&common_dir)))
    }

    pub fn in_git_dir(git_dir: &Path) -> Self {
        Self {
            dir: git_dir.join("wt"),
        }
    }

    pub fn path(&self, file: &str) -> PathBuf {
        self.dir.join("links").join(file)
    }

    fn manifest(&self) -> PathBuf {
        self.dir.join("manifest")
    }

    pub fn list(&self) -> Result<Vec<String>, String> {
        let manifest = self.manifest();
        match fs::read_to_string(&manifest) {
            Ok(content) => Ok(content
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(format!("cannot read {}: {e}", manifest.display())),
        }
    }

    fn save(&self, files: &[String]) -> Result<(), String> {
        let manifest = self.manifest();
        if files.is_empty() {
            return match fs::remove_file(&manifest) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    Err(format!("cannot write {}: {e}", manifest.display()))
                }
                _ => Ok(()),
            };
        }
        fs::create_dir_all(&self.dir)
            .map_err(|e| format!("cannot create {}: {e}", self.dir.display()))?;
        let tmp = manifest.with_extension(format!("tmp.{}", layout::random_id()));
        let content: String = files.iter().map(|f| format!("{f}\n")).collect();
        fs::write(&tmp, content)
            .and_then(|()| fs::rename(&tmp, &manifest))
            .map_err(|e| {
                let _ = fs::remove_file(&tmp);
                format!("cannot write {}: {e}", manifest.display())
            })
    }

    // Records the entry before moving the file, so an interrupted run is finished by the next
    // `wt link` instead of leaving an untracked file in the store.
    pub fn add(&self, file: &str, src: &Path) -> Result<(), String> {
        let mut files = self.list()?;
        let added = !files.iter().any(|f| f == file);
        if added {
            files.push(file.to_owned());
            self.save(&files)?;
        }
        let dest = self.path(file);
        let moved = dest
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| move_path(src, &dest));
        if let Err(e) = moved {
            if added {
                files.pop();
                let _ = self.save(&files);
            }
            return Err(format!(
                "cannot move {file} into {}: {e}",
                terminal::tilde_path(&dest)
            ));
        }
        symlink(&dest, src).map_err(|e| {
            format!(
                "moved {file} into {} but cannot link it back: {e}",
                terminal::tilde_path(&dest)
            )
        })
    }

    pub fn remove(&self, file: &str) -> Result<(), String> {
        let mut files = self.list()?;
        files.retain(|f| f != file);
        self.save(&files)?;
        let path = self.path(file);
        let removed = match path.symlink_metadata() {
            Ok(meta) if meta.is_dir() => fs::remove_dir_all(&path),
            Ok(_) => fs::remove_file(&path),
            Err(_) => Ok(()),
        };
        removed.map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
        let links_dir = self.dir.join("links");
        for dir in path.ancestors().skip(1) {
            if !dir.starts_with(&links_dir) || fs::remove_dir(dir).is_err() {
                break;
            }
        }
        Ok(())
    }
}

pub fn normalize_path(file: &str) -> Result<String, String> {
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
        None => Err(format!("path must name a file or directory: '{file}'")),
        Some(".git") => Err(format!("cannot link git metadata: {file}")),
        Some(_) => Ok(parts.join("/")),
    }
}

pub fn copy_tree(src: &Path, dest: &Path) -> io::Result<()> {
    let meta = src.symlink_metadata()?;
    if meta.is_symlink() {
        symlink(fs::read_link(src)?, dest)
    } else if meta.is_dir() {
        fs::create_dir(dest)?;
        fs::set_permissions(dest, meta.permissions())?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(drop)
    }
}

fn move_path(src: &Path, dest: &Path) -> io::Result<()> {
    match fs::rename(src, dest) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            copy_tree(src, dest)?;
            if src.symlink_metadata()?.is_dir() {
                fs::remove_dir_all(src)
            } else {
                fs::remove_file(src)
            }
        }
        result => result,
    }
}

// Before shared files moved into the store, `~/.wt/config` mapped a source worktree to the files
// it shared. Each entry's files move into that repo's store and links to them are re-pointed;
// entries that fail are kept so the next run retries them.
pub fn migrate_legacy_config() {
    let Ok(path) = layout::wt_home().map(|home| home.join("config")) else {
        return;
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return;
    };
    let mut config = match content.parse::<toml::Table>() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("cannot migrate links from {}: {e}", path.display());
            return;
        }
    };

    let mut kept = toml::Table::new();
    if let Some(toml::Value::Table(entries)) = config.remove("links") {
        for (source, files) in entries {
            let files: Vec<&str> = files
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(toml::Value::as_str)
                .collect();
            let failed = migrate_source(Path::new(&source), &files);
            if !failed.is_empty() {
                kept.insert(source, failed.into());
            }
        }
    }

    let saved = if kept.is_empty() {
        let _ = fs::remove_file(path.with_extension("lock"));
        fs::remove_file(&path)
    } else {
        config.insert("links".into(), kept.into());
        fs::write(&path, config.to_string())
    };
    if let Err(e) = saved {
        eprintln!("cannot update {}: {e}", path.display());
    }
}

fn migrate_source(source: &Path, files: &[&str]) -> Vec<String> {
    if !source.exists() {
        eprintln!(
            "cannot migrate links from {}: worktree not found",
            terminal::tilde_path(source)
        );
        return Vec::new();
    }
    let git = Git::new(source);
    let setup = Store::of(&git).and_then(|store| {
        let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);
        Ok((store, worktrees))
    });
    let (store, worktrees) = match setup {
        Ok(setup) => setup,
        Err(e) => {
            eprintln!(
                "cannot migrate links from {}: {e}",
                terminal::tilde_path(source)
            );
            return files.iter().map(|f| (*f).to_owned()).collect();
        }
    };

    let mut failed = Vec::new();
    for file in files {
        let Ok(file) = normalize_path(file) else {
            continue;
        };
        if let Err(e) = migrate_file(&store, &worktrees, &source.join(&file), &file) {
            eprintln!("cannot migrate {file}: {e}");
            failed.push(file);
        }
    }
    failed
}

fn migrate_file(
    store: &Store,
    worktrees: &[Worktree],
    old: &Path,
    file: &str,
) -> Result<(), String> {
    let is_real = old.symlink_metadata().is_ok_and(|m| !m.is_symlink());
    let shared = store.list()?.iter().any(|f| f == file);
    match (is_real, shared) {
        (true, true) => {
            eprintln!(
                "kept {}: {file} is already shared from another worktree",
                terminal::tilde_path(old)
            );
            return Ok(());
        }
        (true, false) => store.add(file, old)?,
        (false, false) => return Ok(()),
        (false, true) => {}
    }

    // Old links point at the former source, which now links to the store; a target that
    // resolves to the store entry is one of them.
    let entry = store.path(file);
    let canonical_entry = layout::canonicalize_or_self(&entry);
    for wt in worktrees.iter().filter(|wt| !wt.bare && wt.live()) {
        let dest = wt.path.join(file);
        let relinks = fs::read_link(&dest)
            .is_ok_and(|t| t != entry && layout::canonicalize_or_self(&t) == canonical_entry);
        if relinks {
            fs::remove_file(&dest)
                .and_then(|()| symlink(&entry, &dest))
                .map_err(|e| format!("cannot re-link {}: {e}", dest.display()))?;
        }
    }
    eprintln!("moved shared {file} into {}", terminal::tilde_path(&entry));
    Ok(())
}
