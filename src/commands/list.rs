use std::path::Path;

use crate::git::Git;
use crate::layout;
use crate::terminal::{self, Colors, display_width, pad, trunc, trunc_tail};
use crate::worktree::{self, WorktreeInfo};

pub fn run(repo: Option<&Path>, all: bool) -> Result<(), String> {
    if all {
        return run_all();
    }

    let repo_root = Git::find_repo(repo)?;
    let worktrees = worktree::parse_porcelain(&Git::new(&repo_root).list_worktrees()?);
    let mut infos = worktree::enrich_worktrees(&worktrees);
    if let Some(cwd) = layout::current_dir() {
        worktree::mark_current(&mut infos, &cwd);
    }

    let clr = terminal::colors();
    let rows = table_rows(&infos, &clr);
    let widths = Widths::fit(&rows, terminal::width(), 0);
    print_table(&rows, &widths, &clr, "");
    Ok(())
}

fn run_all() -> Result<(), String> {
    let repos = worktree::load_all(&layout::worktrees_root()?);
    let clr = terminal::colors();
    let tables: Vec<_> = repos
        .iter()
        .map(|repo| (&repo.name, table_rows(&repo.worktrees, &clr)))
        .collect();
    let widths = Widths::fit(
        tables.iter().flat_map(|(_, rows)| rows),
        terminal::width(),
        GROUP_INDENT.len(),
    );
    for (i, (name, rows)) in tables.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{}{name}:{}", clr.bold, clr.reset);
        print_table(rows, &widths, &clr, GROUP_INDENT);
    }
    Ok(())
}

const GROUP_INDENT: &str = "  ";
const BRANCH_MIN: usize = 10;
const PATH_MIN: usize = 20;
// "* " marker plus two 3-space column gaps
const FIXED_W: usize = 2 + 3 + 3;

struct Row<'a> {
    current: bool,
    branch: &'a str,
    status: String,
    path: String,
    badges: Vec<(&'static str, &'static str)>,
}

impl Row<'_> {
    fn badges_width(&self) -> usize {
        self.badges.iter().map(|(text, _)| text.len() + 2).sum()
    }
}

fn table_rows<'a>(worktrees: &'a [WorktreeInfo], clr: &Colors) -> Vec<Row<'a>> {
    let mut infos: Vec<&WorktreeInfo> = worktrees.iter().filter(|i| !i.worktree.bare).collect();
    infos.sort_by(|a, b| a.worktree.display_order().cmp(&b.worktree.display_order()));
    infos
        .into_iter()
        .map(|info| {
            let wt = &info.worktree;
            let badges = [
                (wt.detached, "[detached]", clr.dim),
                (wt.locked, "[locked]", clr.bold_yellow),
                (wt.prunable, "[prunable]", clr.red),
            ]
            .into_iter()
            .filter_map(|(on, text, color)| on.then_some((text, color)))
            .collect();
            Row {
                current: info.current,
                branch: wt.branch.as_deref().unwrap_or("(detached)"),
                status: info.status.summary().unwrap_or_else(|| "-".into()),
                path: terminal::tilde_path(&wt.path),
                badges,
            }
        })
        .collect()
}

struct Widths {
    branch: usize,
    status: usize,
    path: usize,
}

impl Widths {
    fn fit<'a>(rows: impl IntoIterator<Item = &'a Row<'a>>, cols: usize, indent: usize) -> Self {
        let (mut branch, mut status, mut path) = ("BRANCH".len(), "STATUS".len(), "PATH".len());
        for row in rows {
            branch = branch.max(display_width(row.branch));
            status = status.max(display_width(&row.status));
            path = path.max(display_width(&row.path) + row.badges_width());
        }
        let avail = cols.saturating_sub(indent + FIXED_W + status);
        let branch = branch.min(avail.saturating_sub(path.min(PATH_MIN)).max(BRANCH_MIN));
        Self {
            branch,
            status,
            path: avail.saturating_sub(branch),
        }
    }
}

fn print_table(rows: &[Row], widths: &Widths, clr: &Colors, indent: &str) {
    let Widths {
        branch: branch_w,
        status: status_w,
        path: path_w,
    } = *widths;
    println!(
        "{indent}  {:<branch_w$}   {:<status_w$}   PATH",
        "BRANCH", "STATUS"
    );
    for row in rows {
        let branch = trunc(row.branch, branch_w);
        let branch_pad = " ".repeat(branch_w.saturating_sub(display_width(&branch)));
        let (marker, branch_color) = if row.current {
            ("*", clr.green)
        } else {
            (" ", "")
        };
        let path = trunc_tail(&row.path, path_w.saturating_sub(row.badges_width()));
        let badges: String = row
            .badges
            .iter()
            .map(|(text, color)| format!("  {color}{text}{}", clr.reset))
            .collect();
        println!(
            "{indent}{marker} {branch_color}{branch}{}{branch_pad}   {}   {path}{badges}",
            clr.reset,
            pad(&trunc(&row.status, status_w), status_w),
        );
    }
}
