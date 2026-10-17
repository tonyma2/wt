use std::collections::BTreeSet;
use std::fs;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io;
use std::path::{Path, PathBuf};

use crate::git::Git;

pub fn wt_home() -> Result<PathBuf, String> {
    std::env::home_dir()
        .map(|home| home.join(".wt"))
        .ok_or_else(|| "cannot determine home directory".into())
}

pub fn worktrees_root() -> Result<PathBuf, String> {
    wt_home().map(|p| p.join("worktrees"))
}

pub fn repos_root() -> Result<PathBuf, String> {
    wt_home().map(|p| p.join("repos"))
}

pub fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn current_dir() -> Option<PathBuf> {
    std::env::current_dir().and_then(|p| p.canonicalize()).ok()
}

pub fn is_cwd_inside(path: &Path, cwd: Option<&Path>) -> bool {
    cwd.is_some_and(|cwd| cwd.starts_with(canonicalize_or_self(path)))
}

// RandomState is seeded from OS randomness; callers retry on collision, so 24 bits is plenty.
pub fn random_id() -> String {
    let bits = RandomState::new().build_hasher().finish();
    format!("{:06x}", bits & 0xff_ffff)
}

pub fn parse_repo_name(url: &str) -> Option<&str> {
    let url = url.trim_end_matches('/');
    url.strip_suffix(".git")
        .unwrap_or(url)
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .filter(|s| !matches!(*s, "" | "." | ".."))
}

pub fn create_dest(repo_root: &Path, git: &Git) -> Result<PathBuf, String> {
    let origin_url = git.remote_url("origin");
    let repo_name = origin_url
        .as_deref()
        .and_then(parse_repo_name)
        .or_else(|| repo_root.to_str().and_then(parse_repo_name))
        .ok_or_else(|| format!("cannot determine repo name from {}", repo_root.display()))?;
    create_worktree_dest(repo_name)
}

pub fn create_worktree_dest(repo_name: &str) -> Result<PathBuf, String> {
    create_unique_dest(&worktrees_root()?, repo_name)
}

pub fn create_bare_dest(repo_name: &str) -> Result<PathBuf, String> {
    create_unique_dest(&repos_root()?, repo_name)
}

fn create_unique_dest(base: &Path, repo_name: &str) -> Result<PathBuf, String> {
    let create_err =
        |path: &Path, e: io::Error| format!("cannot create directory {}: {e}", path.display());
    fs::create_dir_all(base).map_err(|e| create_err(base, e))?;
    let base = canonicalize_or_self(base);
    for _ in 0..10 {
        let id_dir = base.join(random_id());
        match fs::create_dir(&id_dir) {
            Ok(()) => {
                let dest = id_dir.join(repo_name);
                return fs::create_dir(&dest).map(|()| dest.clone()).map_err(|e| {
                    let _ = fs::remove_dir(&id_dir);
                    create_err(&dest, e)
                });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(create_err(&id_dir, e)),
        }
    }
    Err(format!(
        "cannot generate unique path under {}",
        base.display()
    ))
}

pub fn cleanup_dest(dest: &Path) {
    let _ = fs::remove_dir_all(dest);
    if let Some(parent) = dest.parent() {
        let _ = fs::remove_dir(parent);
    }
}

pub fn cleanup_empty_parent(path: &Path, cwd: Option<&Path>) {
    if let Some(parent) = path.parent()
        && is_managed_worktree_dir(parent)
        && !is_cwd_inside(parent, cwd)
    {
        let _ = fs::remove_dir(parent);
    }
}

fn is_managed_worktree_dir(dir: &Path) -> bool {
    let Ok(wt_base) = worktrees_root() else {
        return false;
    };
    canonicalize_or_self(dir).parent() == Some(canonicalize_or_self(&wt_base).as_path())
}

// Each entry is a directory holding a `.git` file, paired with the gitdir it points to
// (`None` when the file cannot be parsed).
pub fn managed_worktrees(wt_root: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut found = Vec::new();
    collect_worktrees(wt_root, &mut found);
    found
}

fn collect_worktrees(dir: &Path, found: &mut Vec<(PathBuf, Option<PathBuf>)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|ft| ft.is_dir()) {
            continue;
        }

        let path = entry.path();
        let dot_git = path.join(".git");

        if dot_git.is_file() {
            let gitdir = parse_gitdir(&dot_git);
            found.push((path, gitdir));
        } else if !dot_git.is_dir() {
            collect_worktrees(&path, found);
        }
    }
}

pub fn admin_repos(managed: &[(PathBuf, Option<PathBuf>)]) -> BTreeSet<PathBuf> {
    managed
        .iter()
        .filter_map(|(_, gitdir)| admin_repo_from_gitdir(gitdir.as_deref()?))
        .collect()
}

fn admin_repo_from_gitdir(gitdir: &Path) -> Option<PathBuf> {
    let worktrees_dir = gitdir.parent()?;
    if worktrees_dir.file_name()? != "worktrees" {
        return None;
    }
    let parent = worktrees_dir.parent()?;
    if parent.file_name()? == ".git" {
        parent.parent().map(Path::to_path_buf)
    } else if parent.join("HEAD").exists() {
        Some(parent.to_path_buf())
    } else {
        None
    }
}

fn parse_gitdir(dot_git_file: &Path) -> Option<PathBuf> {
    let content = fs::read_to_string(dot_git_file).ok()?;
    let gitdir = content.lines().next()?.strip_prefix("gitdir: ")?.trim();
    if gitdir.is_empty() {
        return None;
    }
    let gitdir = Path::new(gitdir);
    if gitdir.is_absolute() {
        Some(gitdir.to_path_buf())
    } else {
        dot_git_file
            .parent()
            .map(|parent| canonicalize_or_self(&parent.join(gitdir)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repo_name_ssh_with_git() {
        assert_eq!(parse_repo_name("git@github.com:org/repo.git"), Some("repo"));
    }

    #[test]
    fn parse_repo_name_https_nested() {
        assert_eq!(
            parse_repo_name("https://gitlab.com/group/sub/project.git"),
            Some("project")
        );
    }

    #[test]
    fn parse_repo_name_https_no_git_suffix() {
        assert_eq!(parse_repo_name("https://github.com/org/repo"), Some("repo"));
    }

    #[test]
    fn parse_repo_name_ssh_no_git_suffix() {
        assert_eq!(parse_repo_name("git@github.com:org/repo"), Some("repo"));
    }

    #[test]
    fn parse_repo_name_azure() {
        assert_eq!(
            parse_repo_name("https://dev.azure.com/org/project/_git/repo"),
            Some("repo")
        );
    }

    #[test]
    fn parse_repo_name_empty() {
        assert_eq!(parse_repo_name(""), None);
    }

    #[test]
    fn parse_repo_name_dot_git_directory() {
        assert_eq!(parse_repo_name("/path/to/repo/.git"), Some("repo"));
        assert_eq!(parse_repo_name("file:///path/to/repo/.git/"), Some("repo"));
    }

    #[test]
    fn parse_repo_name_rejects_dot_components() {
        assert_eq!(parse_repo_name(".."), None);
        assert_eq!(parse_repo_name("https://host/org/."), None);
    }

    #[test]
    fn random_id_is_six_hex_digits_and_varies() {
        let a = random_id();
        assert_eq!(a.len(), 6);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert!((0..10).any(|_| random_id() != a));
    }

    #[test]
    fn create_unique_dest_reserves_fresh_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("worktrees");
        let a = create_unique_dest(&base, "repo").unwrap();
        let b = create_unique_dest(&base, "repo").unwrap();
        assert!(a.is_dir() && b.is_dir());
        assert_ne!(a, b);
        assert_eq!(a.file_name().unwrap(), "repo");
        assert_eq!(
            a.parent().unwrap().parent().unwrap(),
            base.canonicalize().unwrap()
        );
    }

    #[test]
    fn admin_repo_from_gitdir_non_bare() {
        let gitdir = PathBuf::from("/home/user/project/.git/worktrees/feat-branch");
        assert_eq!(
            admin_repo_from_gitdir(&gitdir),
            Some(PathBuf::from("/home/user/project"))
        );
    }

    #[test]
    fn admin_repo_from_gitdir_bare() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("myrepo");
        fs::create_dir(&bare).unwrap();
        fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let wt_gitdir = bare.join("worktrees").join("feat-branch");
        fs::create_dir_all(&wt_gitdir).unwrap();

        assert_eq!(admin_repo_from_gitdir(&wt_gitdir), Some(bare));
    }

    #[test]
    fn admin_repo_from_gitdir_unknown_layout() {
        let gitdir = PathBuf::from("/some/random/worktrees/thing");
        assert_eq!(admin_repo_from_gitdir(&gitdir), None);
    }

    #[test]
    fn parse_gitdir_absolute() {
        let tmp = tempfile::tempdir().unwrap();
        let dot_git = tmp.path().join(".git");
        fs::write(&dot_git, "gitdir: /abs/path/to/.git/worktrees/feat").unwrap();
        let result = parse_gitdir(&dot_git);
        assert_eq!(
            result,
            Some(PathBuf::from("/abs/path/to/.git/worktrees/feat"))
        );
    }

    #[test]
    fn parse_gitdir_relative() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let dot_git = sub.join(".git");
        fs::write(&dot_git, "gitdir: ../../admin/.git/worktrees/feat").unwrap();
        let result = parse_gitdir(&dot_git).unwrap();
        assert_eq!(result, sub.join("../../admin/.git/worktrees/feat"));
    }

    #[test]
    fn parse_gitdir_empty_value() {
        let tmp = tempfile::tempdir().unwrap();
        let dot_git = tmp.path().join(".git");
        fs::write(&dot_git, "gitdir: ").unwrap();
        assert_eq!(parse_gitdir(&dot_git), None);
    }

    #[test]
    fn parse_gitdir_no_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let dot_git = tmp.path().join(".git");
        fs::write(&dot_git, "some random content").unwrap();
        assert_eq!(parse_gitdir(&dot_git), None);
    }

    #[test]
    fn parse_gitdir_missing_file() {
        assert_eq!(parse_gitdir(Path::new("/nonexistent/.git")), None);
    }

    #[test]
    fn managed_worktrees_skips_plain_repos_and_keeps_unparseable() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("plain").join(".git")).unwrap();
        let linked = tmp.path().join("abc123").join("repo");
        fs::create_dir_all(&linked).unwrap();
        fs::write(linked.join(".git"), "gitdir: /admin/.git/worktrees/repo\n").unwrap();
        let broken = tmp.path().join("def456").join("repo");
        fs::create_dir_all(&broken).unwrap();
        fs::write(broken.join(".git"), "garbage\n").unwrap();

        let mut found = managed_worktrees(tmp.path());
        found.sort();
        assert_eq!(
            found,
            [
                (linked, Some(PathBuf::from("/admin/.git/worktrees/repo"))),
                (broken, None),
            ]
        );
        assert_eq!(
            admin_repos(&found),
            BTreeSet::from([PathBuf::from("/admin")])
        );
    }
}
