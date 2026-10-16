use std::path::{Path, PathBuf};
use std::process::Output;

use tempfile::TempDir;

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
fn list_human_output_matches_golden() {
    let (home, repo) = setup();
    let zeta = wt_new(home.path(), &repo, "zeta");
    let alpha = wt_new(home.path(), &repo, "alpha");

    let output = run_list(home.path(), &repo, "200", Some(home.path()));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let expected = format!(
        "  BRANCH   STATUS   PATH\n  \
         main     -        ~/repo\n  \
         alpha    -        {}\n  \
         zeta     -        {}\n",
        tilde(home.path(), &alpha),
        tilde(home.path(), &zeta),
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn aligns_columns_for_wide_branch_names() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "feat/日本語");

    let output = run_list(home.path(), &repo, "200", Some(home.path()));
    assert!(output.status.success());

    let expected = format!(
        "  BRANCH        STATUS   PATH\n  \
         main          -        ~/repo\n  \
         feat/日本語   -        {}\n",
        tilde(home.path(), &wt_path),
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn hides_bare_repo_entry() {
    let (home, _repo, origin) = setup_with_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "-c", "from-bare", "--repo"])
            .arg(&origin);
    });
    assert!(output.status.success());

    let output = run_list(home.path(), &origin, "200", Some(home.path()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("(bare)"), "{stdout}");
    assert!(stdout.contains("from-bare"), "{stdout}");
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
        row.starts_with("* "),
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
    assert!(
        row.contains("   *"),
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
        !main_row.starts_with("* "),
        "main should not be marked as current when cwd is a nested worktree, got: {main_row}",
    );
    let feat_row = find_row(&stdout, "feat-inside");
    assert!(
        feat_row.starts_with("* "),
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
    let data_lines: Vec<&str> = stdout.lines().skip(1).collect();
    for line in &data_lines {
        assert!(
            !line.contains(home_str.as_ref()),
            "path should use ~ instead of raw home, got: {line}",
        );
    }
    let row = find_row(&stdout, "tilde-check");
    assert!(
        row.contains("~/.wt/"),
        "linked worktree path should be tilde-prefixed, got: {row}",
    );
}

fn setup_two_repos() -> (TempDir, PathBuf, PathBuf) {
    let home = TempDir::new().unwrap();
    let repo_a = home.path().join("repo-a");
    std::fs::create_dir(&repo_a).unwrap();
    init_repo(&repo_a);
    let repo_b = home.path().join("repo-b");
    std::fs::create_dir(&repo_b).unwrap();
    init_repo(&repo_b);
    (home, repo_a, repo_b)
}

#[test]
fn list_all_discovers_multiple_repos() {
    let (home, repo_a, repo_b) = setup_two_repos();
    wt_new(home.path(), &repo_a, "feat-a");
    wt_new(home.path(), &repo_b, "feat-b");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--all"]);
        cmd.env("COLUMNS", "200");
    });
    assert!(
        output.status.success(),
        "wt list --all failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("repo-a:"),
        "should contain repo-a header, got:\n{stdout}",
    );
    assert!(
        stdout.contains("repo-b:"),
        "should contain repo-b header, got:\n{stdout}",
    );
    assert!(
        stdout.contains("feat-a"),
        "should contain feat-a branch, got:\n{stdout}",
    );
    assert!(
        stdout.contains("feat-b"),
        "should contain feat-b branch, got:\n{stdout}",
    );
    assert_eq!(
        stdout.matches("BRANCH").count(),
        2,
        "each repo group should have a header row, got:\n{stdout}",
    );
}

#[test]
fn list_all_marks_current_worktree() {
    let (home, repo_a, repo_b) = setup_two_repos();
    let wt_a = wt_new(home.path(), &repo_a, "feat-a");
    wt_new(home.path(), &repo_b, "feat-b");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--all"]);
        cmd.env("COLUMNS", "200");
        cmd.current_dir(&wt_a);
    });
    assert!(
        output.status.success(),
        "wt list --all failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let current_rows: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("* ") || line.starts_with("  * "))
        .collect();
    assert_eq!(
        current_rows.len(),
        1,
        "exactly one row should be marked current, got: {current_rows:?}\n\nfull output:\n{stdout}",
    );
    assert!(
        current_rows[0].contains("feat-a"),
        "feat-a should be the current worktree, got: {}",
        current_rows[0],
    );
}

#[test]
fn list_all_and_repo_are_mutually_exclusive() {
    let (home, repo) = setup();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--all", "--repo"]).arg(&repo);
    });
    assert_exit_code(&output, 2);
}

#[test]
fn list_all_empty_when_no_managed_worktrees() {
    let home = TempDir::new().unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--all"]);
    });
    assert!(output.status.success());
    assert_stdout_empty(&output);
}

#[test]
fn truncates_branch_and_path_in_narrow_terminal() {
    let (home, repo) = setup();
    let long_branch = "feature/very-long-branch-name-for-list-truncation";
    let wt_path = wt_new(home.path(), &repo, long_branch);

    let output = run_list(home.path(), &repo, "72", Some(home.path()));
    assert!(
        output.status.success(),
        "wt list failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = find_row(&stdout, "feature/");
    assert_eq!(
        row.chars().count(),
        72,
        "row should fill the width: {row:?}"
    );
    assert!(
        !row.contains(long_branch),
        "branch should be truncated: {row}"
    );
    assert!(
        row.contains("feature/very-long-branch-name-for-lis…"),
        "{row}"
    );
    let id_dir = wt_path.parent().unwrap().file_name().unwrap();
    assert!(
        row.ends_with(&format!("…rktrees/{}/repo", id_dir.to_string_lossy())),
        "path should keep its tail: {row}"
    );
}

#[test]
fn columns_fit_content_instead_of_truncating() {
    let (home, repo) = setup();
    let branch = "fix/session-timeout-in-login-flow";
    wt_new(home.path(), &repo, branch);

    let output = run_list(home.path(), &repo, "80", Some(home.path()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = find_row(&stdout, "fix/");
    assert!(
        row.contains(branch),
        "branch should fit at 80 columns: {row}"
    );
    assert!(row.trim_end().ends_with("/repo"), "{row}");
}

#[test]
fn list_all_lists_repo_once_with_relative_worktree_paths() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["config", "worktree.useRelativePaths", "true"]);
    wt_new(home.path(), &repo, "a");
    wt_new(home.path(), &repo, "b");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["ls", "--all"]);
    });
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.lines().filter(|l| *l == "repo:").count(),
        1,
        "{stdout}"
    );
}

#[test]
fn piped_output_is_not_truncated() {
    let (home, repo) = setup();
    let branch = format!("feat/{}", "long-".repeat(40));
    let wt_path = wt_new(home.path(), &repo, &branch);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--repo"])
            .arg(&repo)
            .env_remove("COLUMNS");
    });
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = find_row(&stdout, "feat/");
    assert!(row.contains(&branch), "got: {row}");
    assert!(row.contains(&tilde(home.path(), &wt_path)), "got: {row}");
}
