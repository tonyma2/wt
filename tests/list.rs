use std::path::Path;
use std::process::Output;

use serde_json::Value;
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

fn run_list_json(home: &Path, repo: &Path, cwd: Option<&Path>) -> Vec<Value> {
    let output = run_wt(home, |cmd| {
        cmd.args(["list", "--json", "--repo"]).arg(repo);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
    });
    assert!(
        output.status.success(),
        "wt list --json failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<Vec<Value>>(&stdout)
        .unwrap_or_else(|e| panic!("invalid json: {e}\n{stdout}"))
}

fn find_json_entry<'a>(entries: &'a [Value], branch: &str) -> &'a Value {
    entries
        .iter()
        .find(|e| e["branch"].as_str() == Some(branch))
        .unwrap_or_else(|| panic!("no entry with branch '{branch}'"))
}

#[test]
fn json_output_includes_all_fields() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat-json");

    let entries = run_list_json(home.path(), &repo, None);
    assert_eq!(entries.len(), 2);

    let entry = find_json_entry(&entries, "feat-json");
    assert_eq!(entry["name"].as_str(), Some("feat-json"));
    assert!(entry["path"].is_string());
    assert_eq!(entry["branch"].as_str(), Some("feat-json"));
    assert!(entry["head"].is_string());
    assert_eq!(entry["bare"].as_bool(), Some(false));
    assert_eq!(entry["detached"].as_bool(), Some(false));
    assert_eq!(entry["locked"].as_bool(), Some(false));
    assert_eq!(entry["prunable"].as_bool(), Some(false));
    assert_eq!(entry["dirty"].as_bool(), Some(false));
    assert_eq!(entry["current"].as_bool(), Some(false));
    assert!(entry.get("ahead").is_some());
    assert!(entry.get("behind").is_some());
}

#[test]
fn json_shows_dirty_and_current() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "json-dirty");
    std::fs::write(wt_path.join("dirty.txt"), "dirty").unwrap();

    let entries = run_list_json(home.path(), &repo, Some(&wt_path));
    let entry = find_json_entry(&entries, "json-dirty");
    assert_eq!(entry["dirty"].as_bool(), Some(true));
    assert_eq!(entry["current"].as_bool(), Some(true));
}

#[test]
fn json_ahead_behind_for_diverged_branch() {
    let (home, repo, origin) = setup_with_origin();
    let wt_path = wt_new(home.path(), &repo, "json-diverge");
    assert_git_success(&wt_path, &["push", "-u", "origin", "json-diverge"]);
    assert_git_success(&wt_path, &["commit", "--allow-empty", "-m", "local-ahead"]);

    let other = home.path().join("other");
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["clone"]).arg(&origin).arg(&other);
    });
    assert_git_success(&other, &["config", "user.name", "Test"]);
    assert_git_success(&other, &["config", "user.email", "t@t"]);
    assert_git_success(&other, &["checkout", "json-diverge"]);
    assert_git_success(&other, &["commit", "--allow-empty", "-m", "remote-ahead"]);
    assert_git_success(&other, &["push", "origin", "json-diverge"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let entries = run_list_json(home.path(), &repo, Some(home.path()));
    let entry = find_json_entry(&entries, "json-diverge");
    assert_eq!(entry["ahead"].as_u64(), Some(1));
    assert_eq!(entry["behind"].as_u64(), Some(1));
}

#[test]
fn json_null_branch_for_detached() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "json-detach");
    assert_git_success(&wt_path, &["checkout", "--detach"]);

    let entries = run_list_json(home.path(), &repo, Some(home.path()));
    let detached = entries
        .iter()
        .find(|e| e["detached"].as_bool() == Some(true))
        .expect("expected a detached entry");
    assert!(detached["branch"].is_null());
    assert_eq!(detached["name"].as_str(), detached["path"].as_str());
}

#[test]
fn json_null_ahead_behind_without_upstream() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "json-no-upstream");

    let entries = run_list_json(home.path(), &repo, Some(home.path()));
    let entry = find_json_entry(&entries, "json-no-upstream");
    assert!(entry["ahead"].is_null());
    assert!(entry["behind"].is_null());
}

#[test]
fn json_locked_and_prunable() {
    let (home, repo) = setup();
    let wt_locked = wt_new(home.path(), &repo, "json-locked");
    let wt_prunable = wt_new(home.path(), &repo, "json-prunable");

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "lock"]).arg(&wt_locked);
    });
    std::fs::remove_dir_all(&wt_prunable).unwrap();

    let entries = run_list_json(home.path(), &repo, Some(home.path()));

    let locked = find_json_entry(&entries, "json-locked");
    assert_eq!(locked["locked"].as_bool(), Some(true));

    let prunable = entries
        .iter()
        .find(|e| e["prunable"].as_bool() == Some(true))
        .expect("expected a prunable entry");
    assert_eq!(prunable["dirty"].as_bool(), Some(false));
    assert!(prunable["ahead"].is_null());
    assert!(prunable["behind"].is_null());
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

    let header = format!("{:<1} {:<24}   {:<10}   PATH", "", "BRANCH", "STATUS");
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
fn list_all_discovers_multiple_repos() {
    let home = TempDir::new().unwrap();

    let repo_a = home.path().join("repo-a");
    std::fs::create_dir(&repo_a).unwrap();
    init_repo(&repo_a);

    let repo_b = home.path().join("repo-b");
    std::fs::create_dir(&repo_b).unwrap();
    init_repo(&repo_b);

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
    // Each repo group should have its own BRANCH header
    assert_eq!(
        stdout.matches("BRANCH").count(),
        2,
        "each repo group should have a header row, got:\n{stdout}",
    );
}

#[test]
fn list_all_json_includes_repo_field() {
    let home = TempDir::new().unwrap();

    let repo_a = home.path().join("repo-a");
    std::fs::create_dir(&repo_a).unwrap();
    init_repo(&repo_a);

    let repo_b = home.path().join("repo-b");
    std::fs::create_dir(&repo_b).unwrap();
    init_repo(&repo_b);

    wt_new(home.path(), &repo_a, "feat-a");
    wt_new(home.path(), &repo_b, "feat-b");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--all", "--json"]);
    });
    assert!(
        output.status.success(),
        "wt list --all --json failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<Value> =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid json: {e}\n{stdout}"));

    // Should have entries from both repos (2 per repo: main + feature branch)
    assert!(
        entries.len() >= 4,
        "expected at least 4 entries, got {}:\n{stdout}",
        entries.len(),
    );

    // Every entry must have a "repo" field
    for entry in &entries {
        assert!(
            entry["repo"].is_string(),
            "every entry should have a repo field, got: {entry}",
        );
    }

    let repos: std::collections::BTreeSet<&str> =
        entries.iter().filter_map(|e| e["repo"].as_str()).collect();
    assert!(
        repos.contains("repo-a") && repos.contains("repo-b"),
        "should have entries from both repos, got: {repos:?}",
    );
}

#[test]
fn list_all_marks_current_worktree() {
    let home = TempDir::new().unwrap();

    let repo_a = home.path().join("repo-a");
    std::fs::create_dir(&repo_a).unwrap();
    init_repo(&repo_a);

    let repo_b = home.path().join("repo-b");
    std::fs::create_dir(&repo_b).unwrap();
    init_repo(&repo_b);

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
fn list_all_json_empty_when_no_managed_worktrees() {
    let home = TempDir::new().unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--all", "--json"]);
    });
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "[]");
}

#[test]
fn list_single_repo_json_has_no_repo_field() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "no-repo-field");

    let entries = run_list_json(home.path(), &repo, None);
    for entry in &entries {
        assert!(
            entry.get("repo").is_none(),
            "single-repo JSON should not have repo field, got: {entry}",
        );
    }
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
    // At 72 cols: avail=54, branch_w=16, so 50-char branch truncates to 13 chars + "..."
    let truncated = "feature/very-...";
    assert!(
        stdout.contains(truncated),
        "branch should be truncated to '{truncated}', got: {stdout}",
    );
    assert!(
        !stdout.contains(long_branch),
        "full long branch name should not appear in narrow mode, got: {stdout}",
    );
}
