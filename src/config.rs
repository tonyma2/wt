use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    links: BTreeMap<String, Vec<String>>,
}

fn config_path() -> Result<PathBuf, String> {
    layout::wt_home().map(|p| p.join("config"))
}

fn key(worktree: &Path) -> String {
    layout::canonicalize_or_self(worktree)
        .to_string_lossy()
        .into_owned()
}

impl Config {
    pub fn load() -> Result<Self, String> {
        Self::load_from(&config_path()?)
    }

    fn load_from(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content)
                .map(Self::normalized)
                .map_err(|e| format!("cannot parse {}: {e}", path.display())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("cannot read {}: {e}", path.display())),
        }
    }

    fn normalized(mut self) -> Self {
        for files in self.links.values_mut() {
            let mut unique: Vec<String> = Vec::new();
            for file in files.drain(..) {
                let file = normalize_path(&file).unwrap_or(file);
                if !unique.contains(&file) {
                    unique.push(file);
                }
            }
            *files = unique;
        }
        self
    }

    pub fn update(change: impl FnOnce(&mut Self)) -> Result<(), String> {
        Self::update_at(&config_path()?, change)
    }

    fn update_at(path: &Path, change: impl FnOnce(&mut Self)) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let lock_path = path.with_extension("lock");
        let _lock = File::create(&lock_path)
            .and_then(|file| file.lock().map(|()| file))
            .map_err(|e| format!("cannot lock {}: {e}", lock_path.display()))?;
        let mut config = Self::load_from(path)?;
        change(&mut config);
        config.save_to(path)
    }

    fn save_to(&self, path: &Path) -> Result<(), String> {
        let content =
            toml::to_string_pretty(self).map_err(|e| format!("cannot serialize config: {e}"))?;
        let tmp = path.with_extension(format!("tmp.{}", layout::random_id()));
        fs::write(&tmp, content).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("cannot write {}: {e}", path.display())
        })
    }

    pub fn links(&self, worktree: &Path) -> &[String] {
        self.links.get(&key(worktree)).map_or(&[], Vec::as_slice)
    }

    pub fn add_links(&mut self, worktree: &Path, files: &[String]) {
        let existing = self.links.entry(key(worktree)).or_default();
        for file in files {
            if !existing.contains(file) {
                existing.push(file.clone());
            }
        }
    }

    pub fn remove_links(&mut self, worktree: &Path, files: &[String]) {
        let key = key(worktree);
        if let Some(existing) = self.links.get_mut(&key) {
            existing.retain(|f| !files.contains(f));
            if existing.is_empty() {
                self.links.remove(&key);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.links.is_empty());
    }

    #[test]
    fn parse_config_with_links() {
        let toml = r#"
[links]
"/tmp/repo" = [".env", ".env.local"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(
            config.links.get("/tmp/repo"),
            Some(&vec![".env".to_string(), ".env.local".to_string()])
        );
    }

    #[test]
    fn serialize_roundtrip() {
        let mut config = Config::default();
        config.links.insert("/tmp/repo".into(), vec![".env".into()]);
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(
            deserialized.links.get("/tmp/repo"),
            Some(&vec![".env".to_string()])
        );
    }

    #[test]
    fn add_and_remove_links() {
        let mut config = Config::default();
        let wt = Path::new("/nonexistent/wt");
        config.add_links(wt, &[".env".into(), ".env.local".into()]);
        config.add_links(wt, &[".env".into()]);
        assert_eq!(config.links(wt), [".env", ".env.local"]);

        config.remove_links(wt, &[".env".into(), ".env.local".into()]);
        assert!(config.links(wt).is_empty());
        assert!(config.links.is_empty(), "empty entries should be dropped");
    }

    #[test]
    fn normalized_cleans_and_dedups_entries() {
        let toml = r#"
[links]
"/tmp/repo" = ["./.env", ".env", "cfg/", ".git"]
"#;
        let config = toml::from_str::<Config>(toml).unwrap().normalized();
        assert_eq!(
            config.links(Path::new("/tmp/repo")),
            [".env", "cfg", ".git"]
        );
    }

    #[test]
    fn save_creates_config_and_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("config");

        let mut config = Config::default();
        config.links.insert("/tmp/repo".into(), vec![".env".into()]);
        config.save_to(&config_file).unwrap();

        assert!(config_file.exists());
        let content = fs::read_to_string(&config_file).unwrap();
        assert!(content.contains(".env"));

        let temps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("config.tmp."))
            .collect();
        assert!(temps.is_empty(), "no temp files should remain: {temps:?}");
    }

    #[test]
    fn concurrent_updates_keep_every_change() {
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("config");
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let path = config_file.clone();
                std::thread::spawn(move || {
                    Config::update_at(&path, |config| {
                        config.add_links(Path::new(&format!("/repo/{i}")), &[format!("file-{i}")]);
                    })
                    .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let config = Config::load_from(&config_file).unwrap();
        for i in 0..10 {
            assert_eq!(
                config.links(Path::new(&format!("/repo/{i}"))),
                [format!("file-{i}")]
            );
        }
        let temps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("config.tmp."))
            .collect();
        assert!(temps.is_empty(), "no temp files should remain: {temps:?}");
    }
}
