use std::path::PathBuf;

pub mod common;

use common::*;

#[test]
fn prints_worktree_path_for_branch() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "feat-path");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "feat-path", "--repo"]).arg(&repo);
    });

    assert!(
        output.status.success(),
        "wt path failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let reported = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    assert_eq!(
        reported.canonicalize().unwrap(),
        wt_path.canonicalize().unwrap()
    );
    assert_stderr_empty(&output);
}

#[test]
fn errors_when_branch_has_no_worktree() {
    let (home, repo) = setup();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "missing", "--repo"]).arg(&repo);
    });

    assert_error(&output, 1, "no worktree found for: missing\n");
}

#[test]
fn skips_prunable_worktree() {
    let (home, repo) = setup();
    let live_path = wt_new(home.path(), &repo, "feat/stale-path");

    // Create a second worktree for the same branch, then delete it
    let stale_dir = home.path().join(".wt").join("worktrees").join("stale-path");
    std::fs::create_dir_all(&stale_dir).unwrap();
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "--force", "--quiet"])
            .arg(&stale_dir)
            .arg("feat/stale-path");
    });
    std::fs::remove_dir_all(&stale_dir).unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "feat/stale-path", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    let reported = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    assert_eq!(canonical(&reported), canonical(&live_path));
}

#[test]
fn errors_when_branch_name_is_ambiguous() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "shared");

    assert_git_success(&repo, &["symbolic-ref", "HEAD", "refs/heads/shared"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "shared", "--repo"]).arg(&repo);
    });

    let repo_canon = canonical(&repo);
    let wt_canon = canonical(&wt_path);

    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ambiguous name 'shared'; matches:\n"),
        "expected ambiguity header, got: {stderr}",
    );
    assert!(
        stderr.contains(&format!("  - {}\n", repo_canon.display())),
        "expected primary worktree path, got: {stderr}",
    );
    assert!(
        stderr.contains(&format!("  - {}\n", wt_canon.display())),
        "expected linked worktree path, got: {stderr}",
    );
    assert!(
        stderr.contains("multiple worktrees match, specify a path instead\n"),
        "expected ambiguity guidance, got: {stderr}",
    );
}

#[test]
fn resolves_tag_to_detached_head_worktree() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["tag", "v1.0"]);

    let wt_path = wt_checkout(home.path(), &repo, "v1.0");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "v1.0", "--repo"]).arg(&repo);
    });

    assert!(
        output.status.success(),
        "wt path v1.0 should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let reported = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    assert_eq!(canonical(&reported), canonical(&wt_path));
}

#[test]
fn tag_fallback_not_used_when_branch_matches() {
    let (home, repo) = setup();

    let wt_path = wt_new(home.path(), &repo, "v2.0");

    assert_git_success(&repo, &["tag", "v2.0"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "v2.0", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    let reported = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    assert_eq!(canonical(&reported), canonical(&wt_path));
}

#[test]
fn errors_when_ref_matches_multiple_detached_worktrees() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["tag", "v3.0"]);
    assert_git_success(&repo, &["tag", "v3.0-alias"]);

    let _wt1 = wt_checkout(home.path(), &repo, "v3.0");
    let _wt2 = wt_checkout(home.path(), &repo, "v3.0-alias");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "v3.0", "--repo"]).arg(&repo);
    });

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ambiguous ref 'v3.0'"),
        "expected ambiguous ref error, got: {stderr}",
    );
}
