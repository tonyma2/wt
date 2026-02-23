use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub links: BTreeMap<String, Vec<String>>,
}

fn config_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "$HOME is not set".to_string())?;
    Ok(Path::new(&home).join(".wt").join("config"))
}

pub fn load() -> Result<Config, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    toml::from_str(&content).map_err(|e| format!("cannot parse {}: {e}", path.display()))
}

fn save(config: &Config) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let content =
        toml::to_string_pretty(config).map_err(|e| format!("cannot serialize config: {e}"))?;
    std::fs::write(&path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn repo_key(repo: &Path) -> String {
    std::fs::canonicalize(repo)
        .unwrap_or_else(|_| repo.to_path_buf())
        .to_string_lossy()
        .to_string()
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
        .and_then(|config| {
            config
                .links
                .get(&repo_key(repo))
                .cloned()
        })
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
}
