use std::path::Path;
use std::process::Output;

pub mod common;

use common::*;

fn run_list(home: &Path, repo: &Path, columns: &str, cwd: Option<&Path>) -> Output {
    run_wt(home, |cmd| {
        cmd.args(["list", "--repo"]).arg(repo);
        cmd.env("COLUMNS", columns);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
    })
}

fn find_row<'a>(output: &'a str, needle: &str) -> &'a str {
    output
        .lines()
        .skip(1)
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("could not find row containing '{needle}' in:\n{output}"))
}

fn state_col(row: &str) -> &str {
    let prefix = row.rsplit_once("  ").map_or(row, |(left, _)| left);
    prefix.split_whitespace().last().unwrap_or("")
}

#[test]
fn porcelain_matches_git_worktree_list() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat-porcelain");

    let output = wt_bin()
        .args(["list", "--porcelain", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt list --porcelain failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let expected = assert_git_stdout_success(&repo, &["worktree", "list", "--porcelain"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn list_human_output_matches_golden() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat-list");

    let output = run_list(home.path(), &repo, "200", None);
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let normalized = normalize_home_paths(&String::from_utf8_lossy(&output.stdout), home.path());
    let lines: Vec<&str> = normalized.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 lines, got: {normalized}");

    let header = format!(
        "{:<1}  {:<24}  {:<8}  {:<8}  PATH",
        "", "BRANCH", "HEAD", "STATE"
    );
    assert_eq!(lines[0], header);

    assert!(
        lines[1].contains("main") && lines[1].contains("$HOME/repo"),
        "expected main row, got: {}",
        lines[1]
    );
    assert!(
        lines[2].contains("feat-list")
            && lines[2].contains("$HOME/.wt/worktrees/")
            && lines[2].contains("/repo"),
        "expected feat-list row with random path, got: {}",
        lines[2]
    );
}

#[test]
fn marks_current_worktree_when_cwd_is_inside_linked_worktree() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "feat-cwd-marker");
    let nested = wt_path.join("nested");
    std::fs::create_dir(&nested).unwrap();

    let output = run_list(home.path(), &repo, "200", Some(&nested));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let normalized = normalize_home_paths(&String::from_utf8_lossy(&output.stdout), home.path());
    let row = find_row(&normalized, "feat-cwd-marker");
    assert!(
        row.starts_with("*  "),
        "linked worktree should be marked as current, got row: {row}",
    );
}

#[test]
fn shows_dirty_status_for_dirty_worktree() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "dirty-status");
    std::fs::write(wt_path.join("dirty.txt"), "dirty").unwrap();

    let output = run_list(home.path(), &repo, "200", Some(home.path()));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = find_row(&stdout, "dirty-status");
    assert_eq!(state_col(row), "*", "dirty worktree should show '*' state");
}

#[test]
fn shows_ahead_behind_for_diverged_branch() {
    let (home, repo, origin) = setup_with_origin();
    let wt_path = wt_new(home.path(), &repo, "status-diverge");
    assert_git_success(&wt_path, &["push", "-u", "origin", "status-diverge"]);

    assert_git_success(&wt_path, &["commit", "--allow-empty", "-m", "local-ahead"]);

    let other = home.path().join("other");
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["clone"]).arg(&origin).arg(&other);
    });
    assert_git_success(&other, &["config", "user.name", "Test"]);
    assert_git_success(&other, &["config", "user.email", "t@t"]);
    assert_git_success(&other, &["checkout", "status-diverge"]);
    assert_git_success(&other, &["commit", "--allow-empty", "-m", "remote-ahead"]);
    assert_git_success(&other, &["push", "origin", "status-diverge"]);

    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = run_list(home.path(), &repo, "200", Some(home.path()));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = find_row(&stdout, "status-diverge");
    assert_eq!(
        state_col(row),
        "+1-1",
        "diverged branch should show ahead/behind state"
    );
}

#[test]
fn shows_detached_locked_and_prunable_states() {
    let (home, repo) = setup();
    let wt_locked = wt_new(home.path(), &repo, "state-locked");
    let wt_detached = wt_new(home.path(), &repo, "state-detached");
    let wt_prunable = wt_new(home.path(), &repo, "state-prunable");

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "lock"]).arg(&wt_locked);
    });
    assert_git_success(&wt_detached, &["checkout", "--detach"]);
    std::fs::remove_dir_all(&wt_prunable).unwrap();

    let output = run_list(home.path(), &repo, "200", Some(home.path()));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let locked_row = find_row(&stdout, "state-locked");
    assert_eq!(state_col(locked_row), "locked");

    let detached_row = find_row(&stdout, "(detached)");
    assert_eq!(state_col(detached_row), "detached");

    let prunable_row = find_row(&stdout, "state-prunable");
    let state = state_col(prunable_row);
    assert!(
        state.contains("pru"),
        "prunable state should be present (possibly truncated), got: {state}",
    );
}

#[test]
fn truncates_branch_and_path_in_narrow_terminal() {
    let (home, repo) = setup();
    let long_branch = "feature/very-long-branch-name-for-list-truncation";
    wt_new(home.path(), &repo, long_branch);

    let output = run_list(home.path(), &repo, "72", Some(home.path()));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branch_prefix: String = long_branch.chars().take(11).collect();
    let expected_branch = format!("{branch_prefix}...");
    let row = find_row(&stdout, &expected_branch);
    assert!(
        !row.contains(long_branch),
        "branch should be truncated in narrow mode, got row: {row}",
    );
}
