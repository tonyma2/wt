use std::collections::BTreeSet;
use std::path::Path;

use crate::git::{self, Git};
use crate::layout;
use crate::terminal::{self, Colors};
use crate::worktree::{self, Status, Worktree};

enum Msg {
    Removed(String, String),
    WouldRemove(String, String),
    Skipping(String, String),
    Note(String),
    Error(String),
}

impl Msg {
    fn render(&self, clr: &Colors) -> String {
        let (color, verb, target, reason) = match self {
            Msg::Removed(target, reason) => (clr.green, "removed", target, reason),
            Msg::WouldRemove(target, reason) => (clr.yellow, "would remove", target, reason),
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

pub fn run(dry_run: bool) -> Result<(), String> {
    let cwd = layout::current_dir();
    let wt_root = layout::canonicalize_or_self(&layout::worktrees_root()?);
    let mut printer = Printer {
        clr: terminal::stderr_colors(),
        started: false,
    };

    let mut failed = 0usize;
    let managed = layout::managed_worktrees(&wt_root);
    for repo_path in layout::admin_repos(&managed).iter().filter(|p| p.exists()) {
        let mut out = printer.group(Some(terminal::tilde_path(repo_path)));
        match prune_repo(&Git::new(repo_path), dry_run, cwd.as_deref(), &mut out) {
            Ok(0) => {}
            Ok(_) => failed += 1,
            Err(e) => {
                out.emit(Msg::Error(format!(
                    "cannot prune {}: {e}",
                    repo_path.display()
                )));
                failed += 1;
            }
        }
    }

    report_orphans(&wt_root, &mut printer.group(None));

    match failed {
        0 => Ok(()),
        n => Err(format!(
            "cannot prune {n} {}",
            if n == 1 { "repo" } else { "repos" }
        )),
    }
}

fn prune_repo(
    git: &Git,
    dry_run: bool,
    cwd: Option<&Path>,
    out: &mut Group,
) -> Result<usize, String> {
    let worktrees = worktree::parse_porcelain(&git.list_worktrees()?);
    let base = match git.base_ref() {
        Ok(base) => base,
        Err(e) => {
            out.emit(Msg::Error(format!("{e}, skipping merged worktree pruning")));
            return Ok(0);
        }
    };
    let head_ref = git.head_ref();
    let protected: BTreeSet<&str> = [base.strip_prefix("origin/")]
        .into_iter()
        .chain([head_ref.as_deref().and_then(git::branch_name)])
        .flatten()
        .collect();

    // git always lists the main worktree (or the bare repo itself) first
    let candidates: Vec<(&Worktree, &str)> = worktrees
        .iter()
        .skip(1)
        .filter(|wt| !wt.prunable)
        .filter_map(|wt| Some((wt, wt.branch.as_deref()?)))
        .filter(|(_, branch)| !protected.contains(branch))
        .collect();
    if candidates.is_empty() {
        return Ok(0);
    }

    out.emit(Msg::Note("fetching from 'origin'".into()));
    if let Err(e) = git.fetch_remote("origin") {
        out.emit(Msg::Error(format!("{e}, using local refs")));
    }

    let mut failures = 0usize;
    for (wt, branch) in candidates {
        // Also true for a plain merge: the base already contains everything the branch adds.
        if !git.is_squash_merged(&format!("refs/heads/{branch}"), &base) {
            continue;
        }
        let target = branch.to_owned();
        if !pushed(git, branch) {
            out.emit(Msg::Skipping(target, "no upstream".into()));
            continue;
        }

        let blocker = if wt.locked {
            Some("locked")
        } else if layout::is_cwd_inside(&wt.path, cwd) {
            Some("current directory")
        } else if worktree::contains_other_worktree(&worktrees, wt) {
            Some("contains a worktree")
        } else if Status::of(wt).dirty {
            Some("dirty")
        } else {
            None
        };
        if let Some(blocker) = blocker {
            out.emit(Msg::Skipping(target, format!("merged, {blocker}")));
        } else if dry_run {
            out.emit(Msg::WouldRemove(target, "merged".into()));
        } else if let Err(e) = remove(git, wt, branch, cwd) {
            out.emit(Msg::Error(e));
            failures += 1;
        } else {
            out.emit(Msg::Removed(target, "merged".into()));
        }
    }

    Ok(failures)
}

fn pushed(git: &Git, branch: &str) -> bool {
    git.upstream(branch)
        .is_some_and(|upstream| git::branch_name(&upstream) == Some(branch))
        || git
            .remotes_with_branch(branch)
            .is_ok_and(|remotes| !remotes.is_empty())
}

fn remove(git: &Git, wt: &Worktree, branch: &str, cwd: Option<&Path>) -> Result<(), String> {
    git.remove_worktree(&wt.path, false)?;
    layout::cleanup_empty_parent(&wt.path, cwd);
    git.delete_branch(branch)
}

fn report_orphans(wt_root: &Path, out: &mut Group) {
    for (dir, gitdir) in layout::managed_worktrees(wt_root) {
        match gitdir {
            None => out.emit(Msg::Error(format!(
                "cannot parse {}, skipping",
                dir.join(".git").display()
            ))),
            Some(gitdir) if !gitdir.exists() => out.emit(Msg::Skipping(
                terminal::tilde_path(&dir),
                "orphan, not registered with a repo, remove it manually".into(),
            )),
            Some(_) => {}
        }
    }
}
