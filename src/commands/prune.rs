use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::PruneArgs;
use crate::config::Config;
use crate::git::{self, Git};
use crate::layout;
use crate::terminal::{self, Colors};
use crate::worktree::{self, Status, Worktree};

enum Msg {
    Removed(String, String),
    WouldRemove(String, String),
    Repaired(String, String),
    WouldRepair(String, String),
    Skipping(String, String),
    Note(String),
    Error(String),
}

impl Msg {
    fn render(&self, clr: &Colors) -> String {
        let (color, verb, target, reason) = match self {
            Msg::Removed(target, reason) => (clr.green, "removed", target, reason),
            Msg::WouldRemove(target, reason) => (clr.yellow, "would remove", target, reason),
            Msg::Repaired(target, reason) => (clr.green, "repaired", target, reason),
            Msg::WouldRepair(target, reason) => (clr.yellow, "would repair", target, reason),
            Msg::Skipping(target, reason) => (clr.yellow, "skipping", target, reason),
            Msg::Note(text) => return format!("{}{text}{}", clr.dim, clr.reset),
            Msg::Error(text) => return format!("{}{text}{}", clr.red, clr.reset),
        };
        format!(
            "{color}{verb}{} {target} {}({reason}){}",
            clr.reset, clr.dim, clr.reset
        )
    }
}

struct Printer {
    clr: Colors,
    started: bool,
}

impl Printer {
    fn group(&mut self, header: Option<String>) -> Group<'_> {
        Group {
            printer: self,
            header,
            opened: false,
        }
    }
}

struct Group<'a> {
    printer: &'a mut Printer,
    header: Option<String>,
    opened: bool,
}

impl Group<'_> {
    fn emit(&mut self, msg: Msg) {
        let clr = &self.printer.clr;
        if !self.opened {
            if self.printer.started {
                eprintln!();
            }
            if let Some(header) = &self.header {
                eprintln!("{}{header}:{}", clr.bold, clr.reset);
            }
            self.printer.started = true;
            self.opened = true;
        }
        let indent = if self.header.is_some() { "  " } else { "" };
        eprintln!("{indent}{}", msg.render(clr));
    }
}

pub fn run(opts: &PruneArgs) -> Result<(), String> {
    let cwd = layout::current_dir();
    let wt_root = layout::canonicalize_or_self(&layout::worktrees_root()?);
    let managed = layout::managed_worktrees(&wt_root);
    let config = Config::load()?;
    let mut printer = Printer {
        clr: terminal::stderr_colors(),
        started: false,
    };

    if let Some(repo_path) = &opts.repo {
        let root = Git::find_repo(Some(repo_path))?;
        let main = worktree::parse_porcelain(&Git::new(&root).list_worktrees()?)
            .into_iter()
            .next()
            .map_or(root, |wt| wt.path);
        let mut out = printer.group(None);
        return match prune_repo(
            &Git::new(main),
            opts,
            &managed,
            &wt_root,
            cwd.as_deref(),
            &config,
            &mut out,
        )? {
            0 => Ok(()),
            n => Err(format!(
                "cannot clean up {n} {}",
                if n == 1 { "worktree" } else { "worktrees" }
            )),
        };
    }

    let mut failed_repos = 0usize;
    for repo_path in layout::admin_repos(&managed).iter().filter(|p| p.exists()) {
        let mut out = printer.group(Some(terminal::tilde_path(repo_path)));
        let git = Git::new(repo_path);
        match prune_repo(
            &git,
            opts,
            &managed,
            &wt_root,
            cwd.as_deref(),
            &config,
            &mut out,
        ) {
            Ok(0) => {}
            Ok(_) => failed_repos += 1,
            Err(e) => {
                out.emit(Msg::Error(format!(
                    "cannot prune {}: {e}",
                    repo_path.display()
                )));
                failed_repos += 1;
            }
        }
    }

    let mut out = printer.group(None);
    let mut failed_dirs = prune_orphans(&wt_root, opts, cwd.as_deref(), &mut out);
    failed_dirs += prune_empty_dirs(&wt_root, opts, cwd.as_deref(), &mut out);
    failed_dirs += prune_unused_clones(&layout::repos_root()?, opts, cwd.as_deref(), &mut out);

    let mut problems = Vec::new();
    if failed_repos > 0 {
        problems.push(format!(
            "cannot prune {failed_repos} {}",
            if failed_repos == 1 { "repo" } else { "repos" }
        ));
    }
    if failed_dirs > 0 {
        problems.push(format!(
            "cannot remove {failed_dirs} {}",
            if failed_dirs == 1 {
                "directory"
            } else {
                "directories"
            }
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join(", "))
    }
}

fn prune_repo(
    git: &Git,
    opts: &PruneArgs,
    managed: &[(PathBuf, Option<PathBuf>)],
    wt_root: &Path,
    cwd: Option<&Path>,
    config: &Config,
    out: &mut Group,
) -> Result<usize, String> {
    let repaired = repair_moved(git, managed, wt_root, opts.dry_run, out)?;
    let pruned = git.prune_worktrees(opts.dry_run)?;
    for line in pruned.lines() {
        let repaired_entry = repaired
            .iter()
            .any(|name| line.starts_with(&format!("Removing worktrees/{name}:")));
        if !repaired_entry {
            out.emit(Msg::Note(line.to_owned()));
        }
    }
    prune_branches(git, opts, cwd, config, out)
}

// A managed worktree directory that was renamed looks prunable to git; pruning its metadata
// would orphan the directory and the orphan pass would then delete it, uncommitted work included.
fn repair_moved(
    git: &Git,
    managed: &[(PathBuf, Option<PathBuf>)],
    wt_root: &Path,
    dry_run: bool,
    out: &mut Group,
) -> Result<Vec<String>, String> {
    let Some(common_dir) = git.common_dir() else {
        return Ok(Vec::new());
    };
    let common_dir = layout::canonicalize_or_self(&common_dir);

    let mut repaired = Vec::new();
    for (dir, gitdir) in managed {
        let Some(gitdir) = gitdir else { continue };
        let belongs = gitdir
            .parent()
            .and_then(Path::parent)
            .is_some_and(|c| layout::canonicalize_or_self(c) == common_dir);
        if !belongs || !was_moved(gitdir) {
            continue;
        }
        let label = relative_label(dir, wt_root);
        if dry_run {
            out.emit(Msg::WouldRepair(label, "moved".into()));
        } else {
            Git::repair_worktree(dir)?;
            out.emit(Msg::Repaired(label, "moved".into()));
        }
        if let Some(name) = gitdir.file_name() {
            repaired.push(name.to_string_lossy().into_owned());
        }
    }
    Ok(repaired)
}

fn was_moved(gitdir: &Path) -> bool {
    fs::read_to_string(gitdir.join("gitdir"))
        .is_ok_and(|recorded| !gitdir.join(recorded.trim()).exists())
}

struct Candidate<'a> {
    wt: &'a Worktree,
    branch: &'a str,
    upstream: Option<String>,
    remote: Option<String>,
    pushed: bool,
    merged: bool,
}

fn prune_branches(
    git: &Git,
    opts: &PruneArgs,
    cwd: Option<&Path>,
    config: &Config,
    out: &mut Group,
) -> Result<usize, String> {
    let base = resolve_base(git, opts.base.as_deref(), out);
    let base_ref = base.as_deref().and_then(|b| git.full_ref(b));
    let default_ref = opts
        .base
        .as_ref()
        .and_then(|_| git.base_ref().ok())
        .and_then(|b| git.full_ref(&b));
    let head_ref = git.head_ref();
    let protected: BTreeSet<&str> = [
        base_ref.as_deref(),
        default_ref.as_deref(),
        head_ref.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(git::branch_name)
    .chain(opts.base.as_deref())
    .collect();

    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);
    // git always lists the main worktree (or the bare repo itself) first
    let candidates: Vec<Candidate> = worktrees
        .iter()
        .skip(1)
        .filter_map(|wt| {
            let branch = wt.branch.as_deref()?;
            if wt.prunable || protected.contains(branch) {
                return None;
            }
            // Tracking the base itself (e.g. `wt switch x origin/main`) or a local branch (".")
            // doesn't mean the branch was ever pushed.
            let upstream = git.upstream(branch);
            let tracks_base = upstream.is_some() && upstream == base_ref;
            let remote = git
                .upstream_remote(branch)
                .filter(|r| r != "." && !tracks_base);
            let pushed = remote.is_some()
                || git
                    .remotes_with_branch(branch)
                    .is_ok_and(|remotes| !remotes.is_empty());
            let merged = base
                .as_deref()
                .is_some_and(|b| git.is_ancestor(&format!("refs/heads/{branch}"), b));
            Some(Candidate {
                wt,
                branch,
                upstream: upstream.filter(|_| remote.is_some()),
                remote,
                pushed,
                merged,
            })
        })
        .collect();

    let gone_branches = if opts.gone {
        gone_upstreams(git, &candidates, out)
    } else {
        BTreeSet::new()
    };

    let mut failures = 0usize;
    for c in &candidates {
        let pushed = c.pushed;
        // A never-pushed branch is trivially an ancestor of the base right after creation, so
        // "merged" needs an upstream unless the user opted in with --base or --stale.
        let merged = c.merged && (pushed || opts.base.is_some() || opts.stale);
        let gone = gone_branches.contains(c.branch);
        let no_upstream = !pushed && opts.stale;

        let reason = [
            (merged, "merged"),
            (gone, "upstream gone"),
            (no_upstream, "no upstream"),
        ]
        .into_iter()
        .filter_map(|(applies, label)| applies.then_some(label))
        .collect::<Vec<_>>()
        .join(", ");

        let target = c.branch.to_owned();
        if reason.is_empty() {
            if c.merged && !pushed {
                out.emit(Msg::Skipping(target, "no upstream".into()));
            }
            continue;
        }

        // A gone branch whose changes never reached the base (deleted unmerged, or new commits
        // after a squash-merge) exists only locally.
        let unmerged = || {
            !merged
                && !base
                    .as_deref()
                    .is_some_and(|b| git.is_squash_merged(&format!("refs/heads/{}", c.branch), b))
        };
        let blocker = if c.wt.locked {
            Some("locked")
        } else if !config.links(&c.wt.path).is_empty() {
            Some("link source")
        } else if gone && unmerged() {
            Some("not merged")
        } else if layout::is_cwd_inside(&c.wt.path, cwd) {
            Some("current directory")
        } else if Status::of(c.wt).dirty {
            Some("dirty")
        } else {
            None
        };
        if let Some(blocker) = blocker {
            out.emit(Msg::Skipping(target, format!("{reason}, {blocker}")));
        } else if opts.dry_run {
            out.emit(Msg::WouldRemove(target, reason));
        } else if let Err(e) = remove(git, c, cwd) {
            out.emit(Msg::Error(e));
            failures += 1;
        } else {
            out.emit(Msg::Removed(target, reason));
        }
    }

    Ok(failures)
}

fn resolve_base(git: &Git, base: Option<&str>, out: &mut Group) -> Option<String> {
    match base {
        Some(b) if git.rev_parse(b).is_some() => Some(b.to_owned()),
        Some(b) => {
            out.emit(Msg::Error(format!(
                "base branch '{b}' not found, skipping merged worktree pruning"
            )));
            None
        }
        None => git
            .base_ref()
            .inspect_err(|e| out.emit(Msg::Error(format!("{e}, skipping merged worktree pruning"))))
            .ok(),
    }
}

fn gone_upstreams<'a>(
    git: &Git,
    candidates: &'a [Candidate],
    out: &mut Group,
) -> BTreeSet<&'a str> {
    let remotes: BTreeSet<&str> = candidates
        .iter()
        .filter_map(|c| c.remote.as_deref())
        .collect();

    let mut gone = BTreeSet::new();
    for remote in remotes {
        if !git.has_remote(remote) {
            out.emit(Msg::Error(format!(
                "remote '{remote}' not found, skipping upstream-gone pruning"
            )));
            continue;
        }
        out.emit(Msg::Note(format!("fetching from '{remote}'")));
        if let Err(e) = git.fetch_remote(remote) {
            out.emit(Msg::Error(format!("{e}, skipping upstream-gone pruning")));
            continue;
        }
        gone.extend(
            candidates
                .iter()
                .filter(|c| c.remote.as_deref() == Some(remote))
                .filter(|c| {
                    c.upstream
                        .as_deref()
                        .is_some_and(|u| git.rev_parse(u).is_none())
                })
                .map(|c| c.branch),
        );
    }
    gone
}

fn remove(git: &Git, candidate: &Candidate, cwd: Option<&Path>) -> Result<(), String> {
    git.remove_worktree(&candidate.wt.path, false)?;
    layout::cleanup_empty_parent(&candidate.wt.path, cwd);
    git.delete_branch(candidate.branch, true)
}

fn prune_orphans(wt_root: &Path, opts: &PruneArgs, cwd: Option<&Path>, out: &mut Group) -> usize {
    let mut failures = 0usize;
    for (dir, gitdir) in layout::managed_worktrees(wt_root) {
        let label = relative_label(&dir, wt_root);
        match gitdir {
            None => out.emit(Msg::Error(format!(
                "cannot parse {}, skipping",
                dir.join(".git").display()
            ))),
            Some(gitdir) if gitdir.exists() => {}
            Some(_) if layout::is_cwd_inside(&dir, cwd) => {
                out.emit(Msg::Skipping(label, "orphan, current directory".into()));
            }
            Some(_) if !opts.orphans => out.emit(Msg::Skipping(
                label,
                "orphan, repo not found, use --orphans to remove".into(),
            )),
            Some(_) if opts.dry_run => out.emit(Msg::WouldRemove(label, "orphan".into())),
            Some(_) => match fs::remove_dir_all(&dir) {
                Ok(()) => {
                    layout::cleanup_empty_parent(&dir, cwd);
                    out.emit(Msg::Removed(label, "orphan".into()));
                }
                Err(e) => {
                    out.emit(Msg::Error(format!("cannot remove {label}: {e}")));
                    failures += 1;
                }
            },
        }
    }
    failures
}

// Directories under the root that hold nothing but other empty directories: left behind by
// interrupted creates, removed orphans, or `git worktree remove` run outside wt.
fn prune_empty_dirs(
    wt_root: &Path,
    opts: &PruneArgs,
    cwd: Option<&Path>,
    out: &mut Group,
) -> usize {
    let mut failures = 0usize;
    for dir in subdirs(wt_root).into_iter().filter(|d| is_empty_tree(d)) {
        let label = relative_label(&dir, wt_root);
        if layout::is_cwd_inside(&dir, cwd) {
            out.emit(Msg::Skipping(label, "empty, current directory".into()));
        } else if opts.dry_run {
            out.emit(Msg::WouldRemove(label, "empty".into()));
        } else if let Err(e) = fs::remove_dir_all(&dir) {
            out.emit(Msg::Error(format!("cannot remove {label}: {e}")));
            failures += 1;
        } else {
            out.emit(Msg::Removed(label, "empty".into()));
        }
    }
    failures
}

fn is_empty_tree(dir: &Path) -> bool {
    !dir.join(".git").exists()
        && fs::read_dir(dir).is_ok_and(|mut entries| {
            entries.all(|entry| {
                entry.is_ok_and(|e| {
                    e.file_type().is_ok_and(|ft| ft.is_dir()) && is_empty_tree(&e.path())
                })
            })
        })
}

fn prune_unused_clones(
    repos_root: &Path,
    opts: &PruneArgs,
    cwd: Option<&Path>,
    out: &mut Group,
) -> usize {
    let mut failures = 0usize;
    for clone in subdirs(repos_root).iter().flat_map(|id| subdirs(id)) {
        let git = Git::new(&clone);
        let Ok(listing) = git.list_worktrees() else {
            continue;
        };
        if worktree::parse_porcelain(&listing)
            .iter()
            .any(|wt| !wt.bare && !wt.prunable)
        {
            continue;
        }

        let label = terminal::tilde_path(&clone);
        if !git.has_refs("refs/remotes/") {
            out.emit(Msg::Skipping(label, "no worktrees, nothing fetched".into()));
        } else if git.has_unpushed_commits() {
            out.emit(Msg::Skipping(
                label,
                "no worktrees, unpushed commits".into(),
            ));
        } else if layout::is_cwd_inside(&clone, cwd) {
            out.emit(Msg::Skipping(
                label,
                "no worktrees, current directory".into(),
            ));
        } else if opts.dry_run {
            out.emit(Msg::WouldRemove(label, "no worktrees".into()));
        } else if let Err(e) = fs::remove_dir_all(&clone) {
            out.emit(Msg::Error(format!("cannot remove {label}: {e}")));
            failures += 1;
        } else {
            if let Some(id_dir) = clone.parent() {
                let _ = fs::remove_dir(id_dir);
            }
            out.emit(Msg::Removed(label, "no worktrees".into()));
        }
    }
    failures
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn relative_label(path: &Path, wt_root: &Path) -> String {
    path.strip_prefix(wt_root)
        .unwrap_or(path)
        .display()
        .to_string()
}
