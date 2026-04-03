use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::git::Git;
use crate::terminal;

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

pub struct RepoInfo {
    pub name: String,
    pub worktrees: Vec<WorktreeInfo>,
}

pub struct WorktreeInfo {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    pub dirty: bool,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub current: bool,
}

impl WorktreeInfo {
    pub(crate) fn from_worktree(
        wt: &Worktree,
        dirty: bool,
        ahead: Option<u64>,
        behind: Option<u64>,
        current: bool,
    ) -> Self {
        Self {
            path: wt.path.clone(),
            head: wt.head.clone(),
            branch: wt.branch.clone(),
            bare: wt.bare,
            detached: wt.detached,
            locked: wt.locked,
            prunable: wt.prunable,
            dirty,
            ahead,
            behind,
            current,
        }
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

fn find_live_by_branch<'a>(worktrees: &'a [Worktree], name: &str) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|wt| wt.branch.as_deref() == Some(name) && wt.live())
        .collect()
}

fn find_live_by_head<'a>(worktrees: &'a [Worktree], sha: &str) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|wt| wt.detached && wt.head == sha && wt.live())
        .collect()
}

pub enum Resolved<'a> {
    Found(&'a Worktree),
    Ambiguous {
        matches: Vec<&'a Worktree>,
        kind: &'static str,
    },
    NotFound,
}

pub fn resolve_worktree<'a>(worktrees: &'a [Worktree], name: &str, git: &Git) -> Resolved<'a> {
    let matches = find_live_by_branch(worktrees, name);
    if matches.len() == 1 {
        return Resolved::Found(matches[0]);
    }
    if matches.len() > 1 {
        return Resolved::Ambiguous {
            matches,
            kind: "name",
        };
    }

    if let Some(sha) = git.rev_parse(name) {
        let head_matches = find_live_by_head(worktrees, &sha);
        if head_matches.len() == 1 {
            return Resolved::Found(head_matches[0]);
        }
        if head_matches.len() > 1 {
            return Resolved::Ambiguous {
                matches: head_matches,
                kind: "ref",
            };
        }
    }

    Resolved::NotFound
}

pub fn find_by_path<'a>(worktrees: &'a [Worktree], path: &Path) -> Option<&'a Worktree> {
    let canonical = canonicalize_or_self(path);
    worktrees
        .iter()
        .find(|wt| canonicalize_or_self(&wt.path) == canonical)
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
    let canonical = canonicalize_or_self(exclude_path);
    worktrees.iter().any(|wt| {
        wt.branch.as_deref() == Some(branch)
            && canonicalize_or_self(&wt.path) != canonical
            && wt.live()
    })
}

pub(crate) fn random_id() -> Result<String, String> {
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

pub(crate) fn parse_repo_name(url: &str) -> Option<&str> {
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
    create_worktree_dest(repo_name)
}

pub fn cleanup_dest(dest: &Path) {
    let _ = std::fs::remove_dir_all(dest);
    if let Some(parent) = dest.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

pub(crate) fn wt_home() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "cannot determine home directory: HOME is not set".to_string())?;
    Ok(Path::new(&home).join(".wt"))
}

pub(crate) fn worktrees_root() -> Result<PathBuf, String> {
    wt_home().map(|p| p.join("worktrees"))
}

pub(crate) fn repos_root() -> Result<PathBuf, String> {
    wt_home().map(|p| p.join("repos"))
}

pub fn create_bare_dest(repo_name: &str) -> Result<PathBuf, String> {
    let base = repos_root()?;
    let dest = unique_dest(&base, repo_name)?;
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("cannot create directory {}: {e}", dest.display()))?;
    Ok(dest)
}

pub fn create_worktree_dest(repo_name: &str) -> Result<PathBuf, String> {
    let base = worktrees_root()?;
    let dest = unique_dest(&base, repo_name)?;
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("cannot create directory {}: {e}", dest.display()))?;
    Ok(dest)
}

pub(crate) fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn find_primary<'a>(worktrees: &'a [Worktree], repo_root: &Path) -> Option<&'a Worktree> {
    let canonical_root = canonicalize_or_self(repo_root);
    worktrees
        .iter()
        .find(|wt| canonicalize_or_self(&wt.path) == canonical_root)
        .or_else(|| worktrees.iter().find(|wt| !wt.bare))
}

pub fn format_status(bare: bool, dirty: bool, ahead: Option<u64>, behind: Option<u64>) -> String {
    if bare {
        return "bare".into();
    }
    let mut parts: Vec<String> = Vec::new();
    if dirty {
        parts.push("*".into());
    }
    if let Some(a) = ahead
        && a > 0
    {
        parts.push(format!("↑{a}"));
    }
    if let Some(b) = behind
        && b > 0
    {
        parts.push(format!("↓{b}"));
    }
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(" ")
    }
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

fn is_managed_worktree_dir(dir: &Path) -> bool {
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
    let parent = worktrees_dir.parent()?;
    if parent.file_name()?.to_str()? == ".git" {
        let repo = parent.parent()?;
        Some(repo.to_path_buf())
    } else if parent.join("HEAD").exists() {
        Some(parent.to_path_buf())
    } else {
        None
    }
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

pub fn find_current_worktree<'a>(
    worktrees: impl IntoIterator<Item = &'a Worktree>,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    let cwd = cwd?;
    worktrees
        .into_iter()
        .filter(|wt| !wt.prunable)
        .filter_map(|wt| {
            let canonical = canonicalize_or_self(&wt.path);
            cwd.starts_with(&canonical)
                .then_some((wt.path.clone(), canonical))
        })
        .max_by_key(|(_, canonical)| canonical.components().count())
        .map(|(path, _)| path)
}

pub(crate) fn enrich_worktrees(
    worktrees: &[Worktree],
    current_path: Option<&Path>,
) -> Vec<WorktreeInfo> {
    std::thread::scope(|s| {
        let handles: Vec<_> = worktrees
            .iter()
            .map(|wt| {
                s.spawn(move || {
                    let (dirty, ahead, behind) = if wt.bare || wt.prunable {
                        (false, None, None)
                    } else {
                        Git::worktree_status(&wt.path)
                    };
                    let current = current_path == Some(wt.path.as_path());
                    WorktreeInfo::from_worktree(wt, dirty, ahead, behind, current)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    })
}

pub fn load_all() -> Result<Vec<RepoInfo>, String> {
    let wt_root = worktrees_root()?;
    load_all_from(&wt_root)
}

pub(crate) fn load_all_from(wt_root: &Path) -> Result<Vec<RepoInfo>, String> {
    let wt_root = canonicalize_or_self(wt_root);
    let admin_repos = discover_repos(&wt_root);
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok());

    let mut repos: Vec<RepoInfo> = std::thread::scope(|s| {
        let handles: Vec<_> = admin_repos
            .iter()
            .map(|repo_path| {
                s.spawn(move || {
                    let git = Git::new(repo_path);
                    let output = match git.list_worktrees() {
                        Ok(o) => o,
                        Err(e) => {
                            let clr = terminal::stderr_colors();
                            eprintln!(
                                "{}cannot list {}: {e}{}",
                                clr.red,
                                repo_path.display(),
                                clr.reset
                            );
                            return None;
                        }
                    };
                    let worktrees = parse_porcelain(&output);
                    let name = repo_basename(repo_path);
                    let infos = enrich_worktrees(&worktrees, None);
                    if infos.is_empty() {
                        return None;
                    }
                    Some(RepoInfo {
                        name,
                        worktrees: infos,
                    })
                })
            })
            .collect();

        handles
            .into_iter()
            .filter_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });

    if let Some(cwd) = &cwd {
        mark_current(&mut repos, cwd);
    }

    repos.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    Ok(repos)
}

fn mark_current(repos: &mut [RepoInfo], cwd: &Path) {
    let mut best: Option<(usize, usize, usize)> = None;
    for (ri, repo) in repos.iter().enumerate() {
        for (wi, wt) in repo.worktrees.iter().enumerate() {
            if wt.prunable {
                continue;
            }
            let canonical = canonicalize_or_self(&wt.path);
            if cwd.starts_with(&canonical) {
                let depth = canonical.components().count();
                if best.is_none_or(|(_, _, d)| depth > d) {
                    best = Some((ri, wi, depth));
                }
            }
        }
    }
    if let Some((ri, wi, _)) = best {
        repos[ri].worktrees[wi].current = true;
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
        std::fs::create_dir(&bare).unwrap();
        std::fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let wt_gitdir = bare.join("worktrees").join("feat-branch");
        std::fs::create_dir_all(&wt_gitdir).unwrap();

        assert_eq!(admin_repo_from_gitdir(&wt_gitdir), Some(bare));
    }

    #[test]
    fn admin_repo_from_gitdir_unknown_layout() {
        let gitdir = PathBuf::from("/some/random/worktrees/thing");
        assert_eq!(admin_repo_from_gitdir(&gitdir), None);
    }

    fn make_worktree(path: PathBuf, branch: Option<&str>) -> Worktree {
        Worktree {
            path,
            head: "abc123".into(),
            branch: branch.map(String::from),
            bare: false,
            detached: false,
            locked: false,
            prunable: false,
        }
    }

    #[test]
    fn find_by_path_exact() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("project");
        std::fs::create_dir(&dir).unwrap();
        let wts = [make_worktree(dir.clone(), Some("main"))];

        assert!(find_by_path(&wts, &dir).is_some());
        assert!(find_by_path(&wts, Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn find_by_path_through_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let link = tmp.path().join("link");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let wts = [make_worktree(link.clone(), Some("main"))];
        let canonical = real.canonicalize().unwrap();
        assert!(find_by_path(&wts, &canonical).is_some());

        let wts = [make_worktree(canonical, Some("main"))];
        assert!(find_by_path(&wts, &link).is_some());
    }

    #[test]
    fn branch_checked_out_elsewhere_with_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let real_a = tmp.path().join("a");
        let link_a = tmp.path().join("link_a");
        let dir_b = tmp.path().join("b");
        std::fs::create_dir(&real_a).unwrap();
        std::fs::create_dir(&dir_b).unwrap();
        std::os::unix::fs::symlink(&real_a, &link_a).unwrap();

        let wts = [
            make_worktree(link_a.clone(), Some("feat")),
            make_worktree(dir_b, Some("feat")),
        ];

        let canonical_a = real_a.canonicalize().unwrap();
        assert!(branch_checked_out_elsewhere(&wts, "feat", &canonical_a));

        assert!(branch_checked_out_elsewhere(&wts, "feat", &link_a));
        assert!(!branch_checked_out_elsewhere(&wts, "other", &link_a));
    }

    #[test]
    fn format_status_clean() {
        assert_eq!(format_status(false, false, None, None), "-");
        assert_eq!(format_status(false, false, Some(0), Some(0)), "-");
    }

    #[test]
    fn format_status_dirty() {
        assert_eq!(format_status(false, true, None, None), "*");
    }

    #[test]
    fn format_status_ahead_behind() {
        assert_eq!(format_status(false, false, Some(2), None), "↑2");
        assert_eq!(format_status(false, false, None, Some(3)), "↓3");
        assert_eq!(format_status(false, true, Some(1), Some(2)), "* ↑1 ↓2");
    }

    #[test]
    fn format_status_bare() {
        assert_eq!(format_status(true, false, None, None), "bare");
        assert_eq!(format_status(true, true, Some(1), Some(2)), "bare");
    }

    #[test]
    fn resolve_worktree_found_by_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("wt");
        std::fs::create_dir(&dir).unwrap();
        let wts = [make_worktree(dir.clone(), Some("feat"))];
        let git = Git::new("/nonexistent");

        let result = resolve_worktree(&wts, "feat", &git);
        assert!(matches!(result, Resolved::Found(wt) if wt.path == dir));
    }

    #[test]
    fn resolve_worktree_ambiguous_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        let wts = [
            make_worktree(a, Some("feat")),
            make_worktree(b, Some("feat")),
        ];
        let git = Git::new("/nonexistent");

        let result = resolve_worktree(&wts, "feat", &git);
        assert!(
            matches!(result, Resolved::Ambiguous { matches, kind } if matches.len() == 2 && kind == "name")
        );
    }

    #[test]
    fn resolve_worktree_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("wt");
        std::fs::create_dir(&dir).unwrap();
        let wts = [make_worktree(dir, Some("main"))];
        let git = Git::new("/nonexistent");

        let result = resolve_worktree(&wts, "other", &git);
        assert!(matches!(result, Resolved::NotFound));
    }

    fn init_test_repo(dir: &std::path::Path) {
        use std::process::Command;
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(dir)
                    .args(args)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .expect("git failed to start")
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "t@t"]);
        git(&["commit", "--allow-empty", "-m", "init"]);
    }

    #[test]
    fn load_all_from_single_repo() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let admin = tmp.path().join("repos").join("myrepo");
        let wt_root = tmp.path().join("worktrees");

        std::fs::create_dir_all(&admin).unwrap();
        init_test_repo(&admin);

        let wt_dest = wt_root.join("abc123").join("myrepo");
        std::fs::create_dir_all(wt_dest.parent().unwrap()).unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&admin)
                .args(["worktree", "add"])
                .arg(&wt_dest)
                .args(["-b", "feat", "main"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );

        let repos = load_all_from(&wt_root).expect("should load repos");
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "myrepo");
        assert!(!repos[0].worktrees.is_empty());
        assert!(
            repos[0]
                .worktrees
                .iter()
                .any(|wt| wt.branch.as_deref() == Some("feat"))
        );
    }

    #[test]
    fn load_all_from_multiple_repos() {
        use std::process::Command;

        let tmp = tempfile::tempdir().unwrap();
        let wt_root = tmp.path().join("worktrees");
        std::fs::create_dir_all(&wt_root).unwrap();

        for (name, branch) in [("alpha", "feat-a"), ("beta", "feat-b")] {
            let admin = tmp.path().join("repos").join(name);
            std::fs::create_dir_all(&admin).unwrap();
            init_test_repo(&admin);

            let wt_dest = wt_root.join(format!("id-{name}")).join(name);
            std::fs::create_dir_all(wt_dest.parent().unwrap()).unwrap();
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(&admin)
                    .args(["worktree", "add"])
                    .arg(&wt_dest)
                    .args(["-b", branch, "main"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .unwrap()
                    .success()
            );
        }

        let repos = load_all_from(&wt_root).expect("should load repos");
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "alpha");
        assert_eq!(repos[1].name, "beta");
    }

    #[test]
    fn load_all_from_empty_root() {
        let tmp = tempfile::tempdir().unwrap();
        let repos = load_all_from(tmp.path()).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn find_current_worktree_matches_deepest() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("outer");
        let inner = outer.join("inner");
        std::fs::create_dir_all(&inner).unwrap();

        let wts = [
            make_worktree(outer.clone(), Some("main")),
            make_worktree(inner.clone(), Some("feat")),
        ];

        let cwd = inner.canonicalize().unwrap();
        let result = find_current_worktree(&wts, Some(&cwd));
        assert_eq!(result, Some(inner));
    }

    #[test]
    fn find_current_worktree_none_when_cwd_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("project");
        std::fs::create_dir(&dir).unwrap();
        let wts = [make_worktree(dir, Some("main"))];

        assert_eq!(find_current_worktree(&wts, None), None);
    }

    #[test]
    fn find_current_worktree_none_when_cwd_outside() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let other = tmp.path().join("other");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&other).unwrap();
        let wts = [make_worktree(project, Some("main"))];

        let cwd = other.canonicalize().unwrap();
        assert_eq!(find_current_worktree(&wts, Some(&cwd)), None);
    }

    #[test]
    fn find_current_worktree_skips_prunable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("project");
        std::fs::create_dir(&dir).unwrap();

        let wts = [Worktree {
            prunable: true,
            ..make_worktree(dir.clone(), Some("stale"))
        }];

        let cwd = dir.canonicalize().unwrap();
        assert_eq!(find_current_worktree(&wts, Some(&cwd)), None);
    }

    #[test]
    fn mark_current_picks_deepest_match() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("outer");
        let inner = outer.join("inner");
        std::fs::create_dir_all(&inner).unwrap();

        let mut repos = vec![RepoInfo {
            name: "repo".into(),
            worktrees: vec![
                WorktreeInfo::from_worktree(
                    &make_worktree(outer, Some("main")),
                    false,
                    None,
                    None,
                    false,
                ),
                WorktreeInfo::from_worktree(
                    &make_worktree(inner.clone(), Some("feat")),
                    false,
                    None,
                    None,
                    false,
                ),
            ],
        }];

        let cwd = inner.canonicalize().unwrap();
        mark_current(&mut repos, &cwd);
        assert!(!repos[0].worktrees[0].current);
        assert!(repos[0].worktrees[1].current);
    }
}
