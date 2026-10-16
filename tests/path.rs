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

    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ambiguous name 'shared'; matches:\n"),
        "expected ambiguity header, got: {stderr}",
    );
    assert!(
        stderr.contains(&format!("  - {}\n", tilde(home.path(), &repo))),
        "expected primary worktree path, got: {stderr}",
    );
    assert!(
        stderr.contains(&format!("  - {}\n", tilde(home.path(), &wt_path))),
        "expected linked worktree path, got: {stderr}",
    );
    assert!(
        stderr.contains("multiple worktrees match, specify a path instead\n"),
        "expected ambiguity guidance, got: {stderr}",
    );
}

#[test]
fn suggests_close_match_for_typo() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat/login");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "feat/logni", "--repo"]).arg(&repo);
    });

    assert_exit_code(&output, 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean 'feat/login'?"),
        "expected fuzzy suggestion, got: {stderr}",
    );
}

#[test]
fn no_suggestion_for_distant_name() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat/login");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "fix/something-else", "--repo"])
            .arg(&repo);
    });

    assert_error(&output, 1, "no worktree found for: fix/something-else\n");
}

#[test]
fn explains_deleted_worktree_directory() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "gone-dir");
    std::fs::remove_dir_all(&wt_path).unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "gone-dir", "--repo"]).arg(&repo);
    });
    assert_error(
        &output,
        1,
        &format!(
            "worktree for 'gone-dir' was deleted ({}), use `wt switch gone-dir` to recreate it\n",
            tilde(home.path(), &wt_path)
        ),
    );
}
