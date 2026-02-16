use std::path::PathBuf;

pub mod common;

use common::*;

fn wt_switch(home: &std::path::Path, repo: &std::path::Path, name: &str) -> PathBuf {
    let output = run_wt(home, |cmd| {
        cmd.args(["switch", name, "--repo"]).arg(repo);
    });
    assert!(
        output.status.success(),
        "wt switch {name} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    parse_wt_new_path(&output)
}

fn dir_entry_count(dir: &std::path::Path) -> usize {
    if dir.exists() {
        std::fs::read_dir(dir).unwrap().count()
    } else {
        0
    }
}

#[test]
fn switch_returns_existing_worktree_path() {
    let (home, repo) = setup();
    let path = wt_new(home.path(), &repo, "feat/existing");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/existing", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_empty(&output);

    let switch_path = parse_wt_new_path(&output);
    assert_eq!(canonical(&switch_path), canonical(&path));
}

#[test]
fn switch_checks_out_existing_branch() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["branch", "feat/checkout-me"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/checkout-me", "--repo"])
            .arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "wt: checking out 'feat/checkout-me'\n");

    let path = parse_wt_new_path(&output);
    assert!(
        path.exists(),
        "worktree path should exist: {}",
        path.display()
    );
    assert_branch_present(&repo, "feat/checkout-me");
}

#[test]
fn switch_creates_new_branch() {
    let (home, repo) = setup();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/brand-new", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "wt: creating branch 'feat/brand-new'\n");

    let path = parse_wt_new_path(&output);
    assert!(
        path.exists(),
        "worktree path should exist: {}",
        path.display()
    );
    assert_branch_present(&repo, "feat/brand-new");
}

#[test]
fn switch_checks_out_remote_branch() {
    let (home, repo, _origin) = setup_with_origin();

    assert_git_success(&repo, &["branch", "feat/remote-only"]);
    assert_git_success(&repo, &["push", "origin", "feat/remote-only"]);
    assert_git_success(&repo, &["branch", "-D", "feat/remote-only"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/remote-only", "--repo"])
            .arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "wt: checking out 'feat/remote-only'\n");

    let path = parse_wt_new_path(&output);
    assert!(
        path.exists(),
        "worktree path should exist: {}",
        path.display()
    );
}

#[test]
fn switch_is_idempotent() {
    let (home, repo) = setup();
    let first_path = wt_switch(home.path(), &repo, "feat/idem");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/idem", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_empty(&output);

    let second_path = parse_wt_new_path(&output);
    assert_eq!(canonical(&second_path), canonical(&first_path));
}

#[test]
fn switch_errors_on_ambiguous_name() {
    let (home, repo) = setup();

    let wt1_path = wt_new(home.path(), &repo, "feat/ambig");

    // --force bypasses the "already checked out" guard
    let wt2_dir = home.path().join(".wt").join("worktrees").join("manual");
    std::fs::create_dir_all(&wt2_dir).unwrap();
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "--force", "--quiet"])
            .arg(&wt2_dir)
            .arg("feat/ambig");
    });

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/ambig", "--repo"]).arg(&repo);
    });

    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ambiguous"),
        "expected 'ambiguous' in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains("wt rm"),
        "expected actionable guidance in stderr, got: {stderr}",
    );

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "remove", "--force"]).arg(&wt2_dir);
    });
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "remove", "--force"]).arg(&wt1_path);
    });
}

#[test]
fn switch_alias_works() {
    let (home, repo) = setup();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["s", "feat/alias-test", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "wt: creating branch 'feat/alias-test'\n");

    let path = parse_wt_new_path(&output);
    assert!(path.exists(), "worktree path should exist");
}

#[test]
fn switch_cleans_up_on_failure() {
    let (home, repo) = setup();

    let wt_dir = home.path().join(".wt").join("worktrees");
    let before = dir_entry_count(&wt_dir);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "bad:name", "--repo"]).arg(&repo);
    });

    assert!(!output.status.success());
    assert_stdout_empty(&output);

    assert_eq!(
        before,
        dir_entry_count(&wt_dir),
        "no new directories should remain after failed switch",
    );
}
