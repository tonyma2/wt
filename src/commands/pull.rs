use std::collections::{BTreeMap, BTreeSet};

use crate::git::Git;
use crate::layout;
use crate::report::{Msg, Printer};
use crate::terminal;
use crate::worktree::{self, Status, Worktree};

pub enum Outcome {
    Updated(u64),
    UpToDate,
    Skipped(&'static str),
    FetchFailed,
    Failed(String),
}

pub fn run() -> Result<(), String> {
    let mut printer = Printer::new();

    let mut failed = 0usize;
    for repo_path in &worktree::reachable_admin_repos(&layout::worktrees_root()?) {
        let mut out = printer.group(Some(terminal::tilde_path(repo_path)));
        let git = Git::new(repo_path);
        let results = pull_repo(&git, |remote| {
            out.emit(Msg::Note(format!("fetching from '{remote}'")));
            git.fetch_remote(remote)
                .inspect_err(|e| out.emit(Msg::Error(e.clone())))
        });
        let results = match results {
            Ok(results) => results,
            Err(e) => {
                out.emit(Msg::Error(format!(
                    "cannot pull {}: {e}",
                    repo_path.display()
                )));
                failed += 1;
                continue;
            }
        };

        if !results.is_empty()
            && results
                .iter()
                .all(|(_, outcome)| matches!(outcome, Outcome::UpToDate))
        {
            out.emit(Msg::Note("up to date".into()));
        }
        let mut repo_failed = false;
        for (wt, outcome) in results {
            let target = wt.branch.unwrap_or_default();
            match outcome {
                Outcome::UpToDate => {}
                Outcome::Updated(n) => {
                    let commits = if n == 1 { "commit" } else { "commits" };
                    out.emit(Msg::Done("updated", target, format!("{n} {commits}")));
                }
                Outcome::Skipped(reason) => out.emit(Msg::Skipping(target, reason.into())),
                Outcome::FetchFailed => {
                    out.emit(Msg::Skipping(target, "fetch failed".into()));
                    repo_failed = true;
                }
                Outcome::Failed(e) => {
                    out.emit(Msg::Error(format!("cannot pull '{target}': {e}")));
                    repo_failed = true;
                }
            }
        }
        failed += usize::from(repo_failed);
    }

    match failed {
        0 => Ok(()),
        n => Err(format!(
            "cannot pull {n} {}",
            if n == 1 { "repo" } else { "repos" }
        )),
    }
}

// Each tracked remote is fetched once, then clean worktrees that are only behind are
// fast-forwarded. Worktrees without an upstream are left out of the results.
pub fn pull_repo(
    git: &Git,
    mut fetch: impl FnMut(&str) -> Result<(), String>,
) -> Result<Vec<(Worktree, Outcome)>, String> {
    let targets: Vec<(Worktree, String)> = worktree::parse_porcelain(&git.list_worktrees()?)
        .into_iter()
        .filter(|wt| !wt.bare && wt.live())
        .filter_map(|wt| {
            let remote = git.upstream_remote(wt.branch.as_deref()?)?;
            Some((wt, remote))
        })
        .collect();

    // "." is a branch tracking a local branch: nothing to fetch.
    let fetched: BTreeMap<String, bool> = targets
        .iter()
        .map(|(_, remote)| remote.clone())
        .filter(|remote| remote != ".")
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|remote| {
            let ok = fetch(&remote).is_ok();
            (remote, ok)
        })
        .collect();

    Ok(targets
        .into_iter()
        .map(|(wt, remote)| {
            let outcome = if fetched.get(&remote) == Some(&false) {
                Outcome::FetchFailed
            } else {
                fast_forward(&wt)
            };
            (wt, outcome)
        })
        .collect())
}

fn fast_forward(wt: &Worktree) -> Outcome {
    let Some(status) = Status::read(wt) else {
        return Outcome::Failed("cannot read git status".into());
    };
    match (status.ahead, status.behind) {
        (_, None) => Outcome::Skipped("upstream gone"),
        (_, Some(0)) => Outcome::UpToDate,
        (Some(ahead), _) if ahead > 0 => Outcome::Skipped("diverged"),
        _ if status.dirty => Outcome::Skipped("dirty"),
        (_, Some(behind)) => match Git::fast_forward(&wt.path) {
            Ok(()) => Outcome::Updated(behind),
            Err(e) => Outcome::Failed(e),
        },
    }
}
