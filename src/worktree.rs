use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::fuzzy;
use crate::git::Git;
use crate::layout::{self, canonicalize_or_self};
use crate::terminal;

#[derive(Debug, Clone, Default)]
pub struct Worktree {
    pub path: PathBuf,
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

    pub fn label(&self) -> String {
        self.branch
            .as_ref()
            .map_or_else(|| terminal::tilde_path(&self.path), |b| format!("'{b}'"))
    }

    pub fn display_order(&self) -> (u8, &str, &Path) {
        let rank = match self.branch.as_deref() {
            Some("main" | "master") => 0,
            Some(_) => 1,
            None => 2,
        };
        (rank, self.branch.as_deref().unwrap_or(""), &self.path)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Status {
    pub dirty: bool,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
}

impl Status {
    pub fn of(wt: &Worktree) -> Self {
        if wt.bare || wt.prunable {
            return Self::default();
        }
        Git::worktree_status(&wt.path).map_or(
            Self {
                dirty: true,
                ..Self::default()
            },
            |text| Self::parse(&text),
        )
    }

    fn parse(porcelain_v2: &str) -> Self {
        let mut status = Self::default();
        for line in porcelain_v2.lines() {
            if let Some(ab) = line.strip_prefix("# branch.ab ") {
                let mut parts = ab.split_whitespace();
                status.ahead = parts
                    .next()
                    .and_then(|s| s.strip_prefix('+'))
                    .and_then(|s| s.parse().ok());
                status.behind = parts
                    .next()
                    .and_then(|s| s.strip_prefix('-'))
                    .and_then(|s| s.parse().ok());
            } else if !line.starts_with('#') {
                status.dirty = true;
            }
        }
        status
    }

    pub fn summary(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.dirty {
            parts.push("*".to_owned());
        }
        if let Some(a) = self.ahead.filter(|&a| a > 0) {
            parts.push(format!("↑{a}"));
        }
        if let Some(b) = self.behind.filter(|&b| b > 0) {
            parts.push(format!("↓{b}"));
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }
}

pub struct RepoInfo {
    pub name: String,
    pub worktrees: Vec<WorktreeInfo>,
}

pub struct WorktreeInfo {
    pub worktree: Worktree,
    pub status: Status,
    pub current: bool,
}

pub fn parse_porcelain(output: &str) -> Vec<Worktree> {
    output.split("\n\n").filter_map(parse_record).collect()
}

fn parse_record(record: &str) -> Option<Worktree> {
    let mut lines = record.lines();
    let path = lines.next()?.strip_prefix("worktree ")?;
    let mut wt = Worktree {
        path: PathBuf::from(path),
        ..Worktree::default()
    };
    for line in lines {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "branch" => {
                let short = value.strip_prefix("refs/heads/").unwrap_or(value);
                wt.branch = Some(short.to_owned());
            }
            "bare" => wt.bare = true,
            "detached" => wt.detached = true,
            "locked" => wt.locked = true,
            "prunable" => wt.prunable = true,
            _ => {}
        }
    }
    Some(wt)
}

pub fn resolve_worktree<'a>(
    worktrees: &'a [Worktree],
    name: &str,
) -> Result<Option<&'a Worktree>, String> {
    let matches: Vec<_> = worktrees
        .iter()
        .filter(|wt| wt.branch.as_deref() == Some(name) && wt.live())
        .collect();
    match matches.as_slice() {
        [one] => Ok(Some(one)),
        [_, _, ..] => Err(ambiguous_error(name, &matches, "specify a path instead")),
        [] => Ok(None),
    }
}

pub fn ambiguous_error(name: &str, matches: &[&Worktree], hint: &str) -> String {
    let mut msg = format!("ambiguous name '{name}'; matches:\n");
    for m in matches {
        let _ = writeln!(msg, "  - {}", terminal::tilde_path(&m.path));
    }
    let _ = write!(msg, "multiple worktrees match, {hint}");
    msg
}

pub fn not_found_error(name: &str, worktrees: &[Worktree]) -> String {
    if let Some(missing) = worktrees
        .iter()
        .find(|wt| wt.prunable && wt.branch.as_deref() == Some(name))
    {
        return format!(
            "worktree for '{name}' was deleted ({}), use `wt switch {name}` to recreate it",
            terminal::tilde_path(&missing.path)
        );
    }
    let branches: Vec<&str> = worktrees
        .iter()
        .filter_map(|wt| wt.branch.as_deref())
        .collect();
    match fuzzy::close_match(name, &branches) {
        Some(suggestion) => format!("no worktree found for: {name}, did you mean '{suggestion}'?"),
        None => format!("no worktree found for: {name}"),
    }
}

pub fn find_by_path<'a>(worktrees: &'a [Worktree], path: &Path) -> Option<&'a Worktree> {
    let canonical = canonicalize_or_self(path);
    worktrees
        .iter()
        .find(|wt| canonicalize_or_self(&wt.path) == canonical)
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

pub(crate) fn enrich_worktrees(worktrees: &[Worktree]) -> Vec<WorktreeInfo> {
    std::thread::scope(|s| {
        let handles: Vec<_> = worktrees
            .iter()
            .map(|wt| {
                s.spawn(move || WorktreeInfo {
                    status: Status::of(wt),
                    worktree: wt.clone(),
                    current: false,
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    })
}

pub(crate) fn mark_current<'a>(
    worktrees: impl IntoIterator<Item = &'a mut WorktreeInfo>,
    cwd: &Path,
) {
    let deepest = worktrees
        .into_iter()
        .filter(|info| !info.worktree.prunable)
        .filter_map(|info| {
            let canonical = canonicalize_or_self(&info.worktree.path);
            cwd.starts_with(&canonical)
                .then(|| (canonical.components().count(), info))
        })
        .max_by_key(|(depth, _)| *depth);
    if let Some((_, info)) = deepest {
        info.current = true;
    }
}

pub(crate) fn load_all(wt_root: &Path) -> Vec<RepoInfo> {
    let admin_repos =
        layout::admin_repos(&layout::managed_worktrees(&canonicalize_or_self(wt_root)));

    let mut repos: Vec<RepoInfo> = std::thread::scope(|s| {
        let handles: Vec<_> = admin_repos
            .iter()
            .map(|repo_path| s.spawn(move || load_repo(repo_path)))
            .collect();

        handles
            .into_iter()
            .filter_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });

    if let Some(cwd) = layout::current_dir() {
        mark_current(repos.iter_mut().flat_map(|r| &mut r.worktrees), &cwd);
    }

    repos.sort_by(|a, b| a.name.cmp(&b.name));
    repos
}

fn load_repo(repo_path: &Path) -> Option<RepoInfo> {
    let output = Git::new(repo_path)
        .list_worktrees()
        .inspect_err(|e| {
            let clr = terminal::stderr_colors();
            eprintln!(
                "{}cannot list {}: {e}{}",
                clr.red,
                repo_path.display(),
                clr.reset
            );
        })
        .ok()?;
    let worktrees = enrich_worktrees(&parse_porcelain(&output));
    (!worktrees.is_empty()).then(|| RepoInfo {
        name: repo_path.file_name().map_or_else(
            || repo_path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
        worktrees,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn display_order_main_first_then_branches_then_detached() {
        let wt = |branch: Option<&str>, path: &str| Worktree {
            path: PathBuf::from(path),
            branch: branch.map(str::to_owned),
            ..Worktree::default()
        };
        let mut wts = [
            wt(None, "/b"),
            wt(Some("feat/b"), "/c"),
            wt(Some("master"), "/d"),
            wt(Some("feat/a"), "/e"),
            wt(None, "/a"),
            wt(Some("main"), "/f"),
        ];
        wts.sort_by(|a, b| a.display_order().cmp(&b.display_order()));
        let order: Vec<_> = wts.iter().map(|w| w.path.to_str().unwrap()).collect();
        assert_eq!(order, ["/f", "/d", "/e", "/c", "/a", "/b"]);
    }

    #[test]
    fn label_quotes_branch_or_falls_back_to_path() {
        let branch = Worktree {
            path: PathBuf::from("/wt/a"),
            branch: Some("feat/x".into()),
            ..Worktree::default()
        };
        assert_eq!(branch.label(), "'feat/x'");
        let detached = Worktree {
            path: PathBuf::from("/other/wt"),
            detached: true,
            ..Worktree::default()
        };
        assert_eq!(detached.label(), "/other/wt");
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
    fn path_with_spaces() {
        let input = "worktree /home/user/my project\nHEAD abc123\nbranch refs/heads/main\n";
        let wts = parse_porcelain(input);
        assert_eq!(wts[0].path, PathBuf::from("/home/user/my project"));
    }

    #[test]
    fn empty_output() {
        assert!(parse_porcelain("").is_empty());
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
    fn parse_status_clean_with_upstream() {
        let text = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        assert_eq!(
            Status::parse(text),
            Status {
                dirty: false,
                ahead: Some(0),
                behind: Some(0)
            }
        );
    }

    #[test]
    fn parse_status_ahead_behind() {
        let text = "# branch.oid abc123\n# branch.head feat\n# branch.ab +3 -1\n";
        assert_eq!(
            Status::parse(text),
            Status {
                dirty: false,
                ahead: Some(3),
                behind: Some(1)
            }
        );
    }

    #[test]
    fn parse_status_dirty_with_changes() {
        let text = "# branch.oid abc123\n# branch.head main\n# branch.ab +0 -0\n1 .M N... 100644 100644 100644 abc def src/main.rs\n";
        assert!(Status::parse(text).dirty);
    }

    #[test]
    fn parse_status_dirty_untracked() {
        let text = "# branch.oid abc123\n# branch.head main\n? newfile.txt\n";
        assert_eq!(
            Status::parse(text),
            Status {
                dirty: true,
                ..Status::default()
            }
        );
    }

    #[test]
    fn parse_status_no_upstream_or_detached() {
        for text in [
            "# branch.oid abc123\n# branch.head main\n",
            "# branch.oid abc123\n# branch.head (detached)\n",
            "",
        ] {
            assert_eq!(Status::parse(text), Status::default());
        }
    }

    #[test]
    fn status_of_missing_worktree_is_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = make_worktree(tmp.path().join("missing"), Some("feat"));
        assert!(Status::of(&wt).dirty);
    }

    #[test]
    fn summary_clean() {
        assert_eq!(Status::default().summary(), None);
        let synced = Status {
            ahead: Some(0),
            behind: Some(0),
            ..Status::default()
        };
        assert_eq!(synced.summary(), None);
    }

    #[test]
    fn summary_dirty_ahead_behind() {
        let status = |dirty, ahead, behind| Status {
            dirty,
            ahead,
            behind,
        };
        assert_eq!(status(true, None, None).summary().unwrap(), "*");
        assert_eq!(status(false, Some(2), None).summary().unwrap(), "↑2");
        assert_eq!(status(false, None, Some(3)).summary().unwrap(), "↓3");
        assert_eq!(status(true, Some(1), Some(2)).summary().unwrap(), "* ↑1 ↓2");
    }

    fn make_worktree(path: PathBuf, branch: Option<&str>) -> Worktree {
        Worktree {
            path,
            branch: branch.map(String::from),
            ..Worktree::default()
        }
    }

    fn make_info(path: PathBuf, branch: &str) -> WorktreeInfo {
        WorktreeInfo {
            worktree: make_worktree(path, Some(branch)),
            status: Status::default(),
            current: false,
        }
    }

    #[test]
    fn find_by_path_exact() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("project");
        fs::create_dir(&dir).unwrap();
        let wts = [make_worktree(dir.clone(), Some("main"))];

        assert!(find_by_path(&wts, &dir).is_some());
        assert!(find_by_path(&wts, Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn find_by_path_through_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let link = tmp.path().join("link");
        fs::create_dir(&real).unwrap();
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
        fs::create_dir(&real_a).unwrap();
        fs::create_dir(&dir_b).unwrap();
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
    fn resolve_worktree_found_by_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("wt");
        fs::create_dir(&dir).unwrap();
        let wts = [make_worktree(dir.clone(), Some("feat"))];

        let found = resolve_worktree(&wts, "feat").unwrap().unwrap();
        assert_eq!(found.path, dir);
    }

    #[test]
    fn resolve_worktree_ambiguous_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let wts = [
            make_worktree(a.clone(), Some("feat")),
            make_worktree(b.clone(), Some("feat")),
        ];

        let err = resolve_worktree(&wts, "feat").unwrap_err();
        assert_eq!(
            err,
            format!(
                "ambiguous name 'feat'; matches:\n  - {}\n  - {}\nmultiple worktrees match, specify a path instead",
                a.display(),
                b.display()
            )
        );
    }

    #[test]
    fn resolve_worktree_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("wt");
        fs::create_dir(&dir).unwrap();
        let wts = [make_worktree(dir, Some("main"))];

        assert!(resolve_worktree(&wts, "other").unwrap().is_none());
    }

    #[test]
    fn not_found_error_suggests_close_branch() {
        let wts = [make_worktree(PathBuf::from("/wt"), Some("feat/login"))];
        assert_eq!(
            not_found_error("feat/logni", &wts),
            "no worktree found for: feat/logni, did you mean 'feat/login'?"
        );
        assert_eq!(
            not_found_error("unrelated", &wts),
            "no worktree found for: unrelated"
        );
    }

    fn init_test_repo(dir: &Path) {
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

    fn add_worktree(admin: &Path, dest: &Path, branch: &str) {
        use std::process::Command;
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(admin)
                .args(["worktree", "add"])
                .arg(dest)
                .args(["-b", branch, "main"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn load_all_single_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let admin = tmp.path().join("repos").join("myrepo");
        let wt_root = tmp.path().join("worktrees");

        fs::create_dir_all(&admin).unwrap();
        init_test_repo(&admin);

        let wt_dest = wt_root.join("abc123").join("myrepo");
        add_worktree(&admin, &wt_dest, "feat");
        fs::write(wt_dest.join("dirty.txt"), "change").unwrap();

        let repos = load_all(&wt_root);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "myrepo");

        let by_branch = |name: &str| {
            repos[0]
                .worktrees
                .iter()
                .find(|info| info.worktree.branch.as_deref() == Some(name))
                .unwrap_or_else(|| panic!("should have {name} worktree"))
        };
        assert!(by_branch("feat").status.dirty);
        assert!(!by_branch("main").status.dirty);
    }

    #[test]
    fn load_all_multiple_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let wt_root = tmp.path().join("worktrees");
        fs::create_dir_all(&wt_root).unwrap();

        for (name, branch) in [("alpha", "feat-a"), ("beta", "feat-b")] {
            let admin = tmp.path().join("repos").join(name);
            fs::create_dir_all(&admin).unwrap();
            init_test_repo(&admin);
            add_worktree(
                &admin,
                &wt_root.join(format!("id-{name}")).join(name),
                branch,
            );
        }

        let repos = load_all(&wt_root);
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].name, "alpha");
        assert_eq!(repos[1].name, "beta");
    }

    #[test]
    fn load_all_empty_root() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load_all(tmp.path()).is_empty());
    }

    #[test]
    fn mark_current_picks_deepest_match() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("outer");
        let inner = outer.join("inner");
        fs::create_dir_all(&inner).unwrap();

        let mut infos = vec![make_info(outer, "main"), make_info(inner.clone(), "feat")];

        mark_current(&mut infos, &inner.canonicalize().unwrap());
        assert!(!infos[0].current);
        assert!(infos[1].current);
    }

    #[test]
    fn mark_current_picks_deepest_across_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let shallow = tmp.path().join("shallow");
        let deep = shallow.join("nested");
        fs::create_dir_all(&deep).unwrap();

        let mut repos = [
            RepoInfo {
                name: "alpha".into(),
                worktrees: vec![make_info(shallow, "main")],
            },
            RepoInfo {
                name: "beta".into(),
                worktrees: vec![make_info(deep.clone(), "feat")],
            },
        ];

        mark_current(
            repos.iter_mut().flat_map(|r| &mut r.worktrees),
            &deep.canonicalize().unwrap(),
        );
        assert!(
            !repos[0].worktrees[0].current,
            "shallow match in different repo should not be marked"
        );
        assert!(
            repos[1].worktrees[0].current,
            "deepest match across repos should win"
        );
    }

    #[test]
    fn mark_current_none_when_cwd_outside() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let other = tmp.path().join("other");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&other).unwrap();

        let mut infos = vec![make_info(project, "main")];
        mark_current(&mut infos, &other.canonicalize().unwrap());
        assert!(!infos[0].current);
    }

    #[test]
    fn mark_current_skips_prunable() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("project");
        fs::create_dir(&dir).unwrap();

        let mut infos = vec![make_info(dir.clone(), "stale")];
        infos[0].worktree.prunable = true;
        mark_current(&mut infos, &dir.canonicalize().unwrap());
        assert!(!infos[0].current);
    }
}
