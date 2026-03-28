use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::git::Git;

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

impl Worktree {
    pub fn live(&self) -> bool {
        !self.prunable && self.path.exists()
    }
}

#[derive(Default)]
struct PorcelainParser {
    path: Option<PathBuf>,
    head: String,
    branch: Option<String>,
    bare: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
}

impl PorcelainParser {
    fn flush(&mut self, out: &mut Vec<Worktree>) {
        let parser = std::mem::take(self);
        if let Some(path) = parser.path {
            out.push(Worktree {
                path,
                head: parser.head,
                branch: parser.branch,
                bare: parser.bare,
                detached: parser.detached,
                locked: parser.locked,
                prunable: parser.prunable,
            });
        }
    }
}

pub fn parse_porcelain(output: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut parser = PorcelainParser::default();

    for line in output.lines() {
        if line.is_empty() {
            parser.flush(&mut worktrees);
        } else if let Some(rest) = line.strip_prefix("worktree ") {
            parser.flush(&mut worktrees);
            parser.path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            parser.head = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let short = rest.strip_prefix("refs/heads/").unwrap_or(rest);
            parser.branch = Some(short.to_string());
        } else if line == "bare" {
            parser.bare = true;
        } else if line == "detached" {
            parser.detached = true;
        } else if line == "locked" || line.starts_with("locked ") {
            parser.locked = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            parser.prunable = true;
        }
    }

    parser.flush(&mut worktrees);
    worktrees
}

pub fn find_live_by_branch<'a>(worktrees: &'a [Worktree], name: &str) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|wt| wt.branch.as_deref() == Some(name) && wt.live())
        .collect()
}

pub fn find_live_by_head<'a>(worktrees: &'a [Worktree], sha: &str) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|wt| wt.detached && wt.head == sha && wt.live())
        .collect()
}

pub fn find_by_path<'a>(worktrees: &'a [Worktree], path: &Path) -> Option<&'a Worktree> {
    worktrees.iter().find(|wt| wt.path == path)
}

pub fn is_cwd_inside(path: &Path, cwd: Option<&Path>) -> bool {
    let Some(cwd) = cwd else { return false };
    let canonical = canonicalize_or_self(path);
    cwd.starts_with(&canonical)
}

pub fn branch_checked_out_elsewhere(
    worktrees: &[Worktree],
    branch: &str,
    exclude_path: &Path,
) -> bool {
    worktrees
        .iter()
        .any(|wt| wt.branch.as_deref() == Some(branch) && wt.path != exclude_path && wt.live())
}

fn random_id() -> Result<String, String> {
    let mut buf = [0u8; 3];
    getrandom::fill(&mut buf).map_err(|e| format!("cannot generate random id: {e}"))?;
    Ok(format!("{:02x}{:02x}{:02x}", buf[0], buf[1], buf[2]))
}

fn unique_dest(wt_base: &Path, repo_name: &str) -> Result<PathBuf, String> {
    for _ in 0..10 {
        let id = random_id()?;
        let candidate = wt_base.join(id).join(repo_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("cannot generate unique worktree path".into())
}

fn parse_repo_name(url: &str) -> Option<&str> {
    let url = url.trim_end_matches('/');
    url.strip_suffix(".git")
        .unwrap_or(url)
        .rsplit(['/', ':'])
        .next()
        .filter(|s| !s.is_empty())
}

pub fn create_dest(repo_root: &Path, git: &Git) -> Result<PathBuf, String> {
    let origin_url = git.remote_url("origin");
    let repo_name = origin_url
        .as_deref()
        .and_then(parse_repo_name)
        .or_else(|| repo_root.file_name().and_then(|n| n.to_str()))
        .ok_or_else(|| format!("cannot determine repo name from {}", repo_root.display()))?;
    let wt_base = worktrees_root()?;
    let dest = unique_dest(&wt_base, repo_name)?;
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("cannot create directory {}: {e}", dest.display()))?;
    Ok(dest)
}

pub fn cleanup_dest(dest: &Path) {
    let _ = std::fs::remove_dir_all(dest);
    if let Some(parent) = dest.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

pub(crate) fn worktrees_root() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "cannot determine home directory: HOME is not set".to_string())?;
    Ok(Path::new(&home).join(".wt").join("worktrees"))
}

pub(crate) fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn cleanup_empty_parent(path: &Path, cwd: Option<&Path>) {
    if let Some(parent) = path.parent()
        && is_managed_worktree_dir(parent)
        && !is_cwd_inside(parent, cwd)
        && std::fs::read_dir(parent).is_ok_and(|mut d| d.next().is_none())
    {
        let _ = std::fs::remove_dir(parent);
    }
}

pub fn is_managed_worktree_dir(dir: &Path) -> bool {
    let Ok(wt_base) = worktrees_root() else {
        return false;
    };
    let canonical_wt_base = canonicalize_or_self(&wt_base);
    let canonical_dir = canonicalize_or_self(dir);
    canonical_dir.parent() == Some(canonical_wt_base.as_path())
}

pub(crate) fn discover_repos(wt_root: &Path) -> BTreeSet<PathBuf> {
    let mut repos = BTreeSet::new();
    collect_repos(wt_root, &mut repos);
    repos
}

fn collect_repos(dir: &Path, repos: &mut BTreeSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }

        let path = entry.path();
        let dot_git = path.join(".git");

        if dot_git.is_file() {
            if let Some(gitdir) = parse_gitdir(&dot_git)
                && let Some(admin) = admin_repo_from_gitdir(&gitdir)
            {
                repos.insert(admin);
            }
        } else if !dot_git.is_dir() {
            collect_repos(&path, repos);
        }
    }
}

fn admin_repo_from_gitdir(gitdir: &Path) -> Option<PathBuf> {
    let worktrees_dir = gitdir.parent()?;
    if worktrees_dir.file_name()?.to_str()? != "worktrees" {
        return None;
    }
    let dot_git_dir = worktrees_dir.parent()?;
    if dot_git_dir.file_name()?.to_str()? != ".git" {
        return None;
    }
    let repo = dot_git_dir.parent()?;
    Some(repo.to_path_buf())
}

pub(crate) fn repo_basename(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub(crate) fn parse_gitdir(dot_git_file: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(dot_git_file).ok()?;
    let line = content.lines().next()?;
    let gitdir = line.strip_prefix("gitdir: ")?.trim();
    if gitdir.is_empty() {
        return None;
    }
    let gitdir_path = PathBuf::from(gitdir);

    if gitdir_path.is_absolute() {
        Some(gitdir_path)
    } else {
        let parent = dot_git_file.parent()?;
        Some(parent.join(gitdir_path))
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
    fn basic_worktree() {
        let input = "\
worktree /home/user/project
HEAD abc123def456
branch refs/heads/main
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].path, PathBuf::from("/home/user/project"));
        assert_eq!(wts[0].head, "abc123def456");
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert!(!wts[0].bare);
        assert!(!wts[0].detached);
        assert!(!wts[0].locked);
        assert!(!wts[0].prunable);
    }

    #[test]
    fn bare_worktree() {
        let input = "\
worktree /home/user/project.git
HEAD 0000000000000000000000000000000000000000
bare
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert!(wts[0].bare);
        assert!(wts[0].branch.is_none());
    }

    #[test]
    fn detached_head() {
        let input = "\
worktree /home/user/project
HEAD abc123
detached
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert!(wts[0].detached);
        assert!(wts[0].branch.is_none());
    }

    #[test]
    fn multiple_worktrees() {
        let input = "\
worktree /home/user/project
HEAD abc123
branch refs/heads/main

worktree /home/user/.wt/worktrees/a3f2b1/project
HEAD def456
branch refs/heads/feature
locked

";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 2);
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert!(!wts[0].locked);
        assert_eq!(wts[1].branch.as_deref(), Some("feature"));
        assert!(wts[1].locked);
    }

    #[test]
    fn no_trailing_blank_line() {
        let input = "\
worktree /home/user/project
HEAD abc123
branch refs/heads/main

worktree /home/user/.wt/worktrees/a3f2b1/project
HEAD def456
branch refs/heads/feature";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 2);
        assert_eq!(wts[1].branch.as_deref(), Some("feature"));
    }

    #[test]
    fn locked_with_reason() {
        let input = "\
worktree /home/user/project
HEAD abc123
branch refs/heads/main
locked because: in use
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert!(wts[0].locked);
    }

    #[test]
    fn prunable_with_reason() {
        let input = "\
worktree /home/user/project
HEAD abc123
branch refs/heads/main
prunable gitdir file points to non-existent location
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert!(wts[0].prunable);
    }
}
