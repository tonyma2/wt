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

    assert_error(&output, 1, "wt: no worktree found for branch: missing\n");
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
        stderr.contains("wt: ambiguous name 'shared'; matches:\n"),
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
        stderr.contains("wt: multiple worktrees match; specify the full branch name\n"),
        "expected ambiguity guidance, got: {stderr}",
    );
}
