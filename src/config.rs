use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub links: BTreeMap<String, Vec<String>>,
}

fn config_path() -> Result<PathBuf, String> {
    crate::worktree::wt_home().map(|p| p.join("config"))
}

pub fn load() -> Result<Config, String> {
    let path = config_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            toml::from_str(&content).map_err(|e| format!("cannot parse {}: {e}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

fn save(config: &Config) -> Result<(), String> {
    save_to(config, &config_path()?)
}

fn save_to(config: &Config, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let content =
        toml::to_string_pretty(config).map_err(|e| format!("cannot serialize config: {e}"))?;
    let id = crate::worktree::random_id()?;
    let tmp = path.with_extension(format!("tmp.{id}"));
    std::fs::write(&tmp, &content).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot write {}: {e}", path.display())
    })
}

pub(crate) fn repo_key(repo: &Path) -> String {
    crate::worktree::canonicalize_or_self(repo)
        .to_string_lossy()
        .into_owned()
}

pub fn add_links(repo: &Path, files: &[String]) -> Result<(), String> {
    let mut config = load()?;
    let key = repo_key(repo);
    let existing = config.links.entry(key).or_default();
    for file in files {
        if !existing.contains(file) {
            existing.push(file.clone());
        }
    }
    save(&config)
}

pub fn remove_links(repo: &Path, files: &[String]) -> Result<(), String> {
    let mut config = load()?;
    let key = repo_key(repo);
    if let Some(existing) = config.links.get_mut(&key) {
        existing.retain(|f| !files.contains(f));
        if existing.is_empty() {
            config.links.remove(&key);
        }
    }
    save(&config)
}

pub fn get_links(repo: &Path) -> Vec<String> {
    load()
        .ok()
        .and_then(|config| config.links.get(&repo_key(repo)).cloned())
        .unwrap_or_default()
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
    fn save_creates_config_and_cleans_temp() {
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("config");

        let mut config = Config::default();
        config.links.insert("/tmp/repo".into(), vec![".env".into()]);
        save_to(&config, &config_file).unwrap();

        assert!(config_file.exists());
        let content = std::fs::read_to_string(&config_file).unwrap();
        assert!(content.contains(".env"));

        let temps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("config.tmp."))
            .collect();
        assert!(temps.is_empty(), "no temp files should remain: {temps:?}");
    }

    #[test]
    fn concurrent_saves_produce_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_file = dir.path().join("config");

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let path = config_file.clone();
                std::thread::spawn(move || {
                    let mut config = Config::default();
                    config
                        .links
                        .insert(format!("/repo/{i}"), vec![format!("file-{i}")]);
                    save_to(&config, &path).unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let content = std::fs::read_to_string(&config_file).unwrap();
        let config: Config = toml::from_str(&content).unwrap();
        assert_eq!(config.links.len(), 1, "last writer wins: exactly one entry");

        let temps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("config.tmp."))
            .collect();
        assert!(temps.is_empty(), "no temp files should remain: {temps:?}");
    }
}
