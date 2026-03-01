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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 lines, got: {stdout}");

    // New header: no HEAD column, STATUS instead of STATE
    let header = format!("{:<1}  {:<24}  {:<10}  PATH", "", "BRANCH", "STATUS");
    assert_eq!(lines[0], header);

    assert!(
        lines[1].contains("main") && lines[1].contains("~/repo"),
        "expected main row with tilde path, got: {}",
        lines[1]
    );
    assert!(
        lines[2].contains("feat-list") && lines[2].contains("~/.wt/"),
        "expected feat-list row with tilde worktree path, got: {}",
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = find_row(&stdout, "feat-cwd-marker");
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
    // dirty marker appears in STATUS column (after branch separator)
    assert!(
        row.contains("  *"),
        "dirty worktree should show '*' in STATUS column, got: {row}"
    );
}

#[test]
fn shows_ahead_behind_arrows_for_diverged_branch() {
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
    assert!(
        row.contains("↑1"),
        "diverged branch should show ↑ for ahead, got: {row}"
    );
    assert!(
        row.contains("↓1"),
        "diverged branch should show ↓ for behind, got: {row}"
    );
}

#[test]
fn shows_detached_locked_and_prunable_as_badges() {
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
    assert!(
        locked_row.contains("[locked]"),
        "locked worktree should show [locked] badge, got: {locked_row}",
    );

    let detached_row = find_row(&stdout, "(detached)");
    assert!(
        detached_row.contains("[detached]"),
        "detached worktree should show [detached] badge, got: {detached_row}",
    );

    let prunable_row = find_row(&stdout, "state-prunable");
    assert!(
        prunable_row.contains("[prunable]"),
        "prunable worktree should show [prunable] badge, got: {prunable_row}",
    );
}

#[test]
fn does_not_mark_main_as_current_when_cwd_is_nested_worktree_inside_repo() {
    let (home, repo) = setup();
    // Create a worktree nested inside the repo directory — its path will start
    // with the repo root, so a naive starts_with check incorrectly matches main.
    let inside = repo.join(".worktrees").join("feat-inside");
    std::fs::create_dir_all(&inside).unwrap();
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "-b", "feat-inside"])
            .arg(&inside);
    });

    let output = run_list(home.path(), &repo, "200", Some(&inside));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let main_row = find_row(&stdout, "main");
    assert!(
        !main_row.starts_with("*  "),
        "main should not be marked as current when cwd is a nested worktree, got: {main_row}",
    );
    let feat_row = find_row(&stdout, "feat-inside");
    assert!(
        feat_row.starts_with("*  "),
        "feat-inside should be marked as current, got: {feat_row}",
    );
}

#[test]
fn paths_use_tilde_for_home_directory() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "tilde-check");

    let output = run_list(home.path(), &repo, "200", None);
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let home_str = home.path().to_string_lossy();
    // The home path itself should not appear in the output (it should be ~-substituted)
    let data_lines: Vec<&str> = stdout.lines().skip(1).collect();
    for line in &data_lines {
        assert!(
            !line.contains(home_str.as_ref()),
            "path should use ~ instead of raw home, got: {line}",
        );
    }
    // The linked worktree path should start with ~/
    let row = find_row(&stdout, "tilde-check");
    assert!(
        row.contains("~/.wt/"),
        "linked worktree path should be tilde-prefixed, got: {row}",
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
    // With no HEAD column the branch gets a bit more room, but still truncates
    assert!(
        stdout.contains("..."),
        "long branch or path should be truncated in narrow mode, got: {stdout}",
    );
    assert!(
        !stdout.contains(long_branch),
        "full long branch name should not appear in narrow mode, got: {stdout}",
    );
}
