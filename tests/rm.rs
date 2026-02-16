pub mod common;

use common::*;

#[test]
fn removes_worktree_and_branch() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "test-branch");

    let output = wt_bin()
        .args(["rm", "test-branch", "--force", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "test-branch");
}

#[test]
fn resolves_branch_when_local_dir_exists() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "docs");

    std::fs::create_dir(repo.join("docs")).unwrap();

    let output = wt_bin()
        .args(["rm", "docs", "--force", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should resolve branch despite local dir: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "docs");
}

#[test]
fn refuses_dirty_without_force() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "dirty-branch");

    std::fs::write(wt_path.join("uncommitted.txt"), "changes").unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["rm", "dirty-branch", "--repo"]).arg(&repo);
    });
    assert_error(
        &output,
        1,
        "wt: worktree has local changes; use --force to remove\n",
    );
    assert!(wt_path.exists());
}

#[test]
fn removes_branch_without_remote() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "no-remote-branch");

    let output = wt_bin()
        .args(["rm", "no-remote-branch", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should succeed without remote: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "no-remote-branch");
}

#[test]
fn refuses_unmerged_branch_without_remote() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "local-unmerged");

    std::fs::write(wt_path.join("new.txt"), "change").unwrap();
    assert_git_success(&wt_path, &["add", "new.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "local change"]);

    let output = wt_bin()
        .args(["rm", "local-unmerged", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert_error(
        &output,
        1,
        "wt: branch 'local-unmerged' has unpushed commits; use --force to remove\n",
    );
    assert!(wt_path.exists());
}

#[test]
fn refuses_unmerged_branch_even_if_other_local_branch_contains_it() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "local-unmerged-contained");

    std::fs::write(wt_path.join("new.txt"), "change").unwrap();
    assert_git_success(&wt_path, &["add", "new.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "local change"]);
    let tip = assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    assert!(!tip.is_empty());

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["branch", "backup-tip"]).arg(&tip);
    });

    let output = wt_bin()
        .args(["rm", "local-unmerged-contained", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert_error(
        &output,
        1,
        "wt: branch 'local-unmerged-contained' has unpushed commits; use --force to remove\n",
    );
    assert!(wt_path.exists());
}

#[test]
fn removes_branch_when_remote_upstream_was_deleted() {
    let (home, repo) = setup();
    let origin = home.path().join("origin.git");

    init_bare_repo(&origin);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);

    let wt_path = wt_new(home.path(), &repo, "stale-clean");

    assert_git_success(&wt_path, &["push", "-u", "origin", "stale-clean"]);
    assert_git_success(&repo, &["push", "origin", "--delete", "stale-clean"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["rm", "stale-clean", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should allow clean branch after upstream deletion: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "stale-clean");
}

#[test]
fn removing_multiple_targets_reports_failures_and_removes_successes() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "remove-me");

    let output = wt_bin()
        .args(["rm", "remove-me", "missing-branch", "--force", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert_exit_code(&output, 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no worktree found for branch: missing-branch"),
        "should report failing target, got: {stderr}",
    );
    assert!(
        stderr.contains("1 worktree(s) could not be removed"),
        "should report aggregate failure count, got: {stderr}",
    );
    assert!(
        !wt_path.exists(),
        "successful target should still be removed when another target fails"
    );
    assert_branch_absent(&repo, "remove-me");
}

#[test]
fn rejects_ambiguous_branch_name() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "shared");
    assert_git_success(&repo, &["symbolic-ref", "HEAD", "refs/heads/shared"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["rm", "shared", "--force", "--repo"]).arg(&repo);
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
        stderr.contains("wt: multiple worktrees match; specify a path instead\n"),
        "expected ambiguity guidance, got: {stderr}",
    );
    assert!(wt_path.exists());
}

#[test]
fn removes_worktree_by_path() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "remove-by-path");

    let output = wt_bin()
        .arg("rm")
        .arg(&wt_path)
        .arg("--force")
        .current_dir(home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should remove a worktree by path: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "remove-by-path");
}

#[test]
fn removes_worktree_by_path_with_repo_flag() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "remove-by-path-repo");

    let output = wt_bin()
        .arg("rm")
        .arg(&wt_path)
        .arg("--force")
        .arg("--repo")
        .arg(&repo)
        .current_dir(home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should remove a worktree by path with --repo: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "remove-by-path-repo");
}

#[test]
fn rejects_non_root_worktree_path() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "non-root-path");
    let nested = wt_path.join("nested");
    std::fs::create_dir(&nested).unwrap();

    let output = wt_bin()
        .arg("rm")
        .arg(&nested)
        .arg("--force")
        .current_dir(home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt rm should reject non-root paths"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a worktree root"),
        "should explain non-root path rejection, got: {stderr}",
    );
    assert!(wt_path.exists());
}

#[test]
fn cannot_remove_primary_worktree_by_branch() {
    let (_home, repo) = setup();

    let output = wt_bin()
        .args(["rm", "main", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt rm should refuse removing the primary worktree"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot remove the primary worktree"),
        "expected primary-worktree error, got: {stderr}",
    );
    assert!(repo.exists());
}

#[test]
fn cannot_remove_primary_worktree_by_path() {
    let (home, repo) = setup();

    let output = wt_bin()
        .arg("rm")
        .arg(&repo)
        .arg("--force")
        .current_dir(home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt rm should refuse removing the primary worktree by path"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot remove the primary worktree"),
        "expected primary-worktree error, got: {stderr}",
    );
    assert!(repo.exists());
}

#[test]
fn refuses_when_current_directory_is_inside_target_worktree() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "cwd-guard");

    let output = wt_bin()
        .args(["rm", "cwd-guard", "--force", "--repo"])
        .arg(&repo)
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt rm should refuse when cwd is inside target worktree"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("current directory is inside the worktree"),
        "expected cwd guard error, got: {stderr}",
    );
    assert!(wt_path.exists());
}

#[test]
fn force_removes_dirty_worktree() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "dirty-force");
    std::fs::write(wt_path.join("dirty.txt"), "dirty").unwrap();

    let output = wt_bin()
        .args(["rm", "dirty-force", "--force", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm --force should remove dirty worktree: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "dirty-force");
}

#[test]
fn force_removes_unmerged_branch() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "unmerged-force");
    std::fs::write(wt_path.join("new.txt"), "change").unwrap();
    assert_git_success(&wt_path, &["add", "new.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "local change"]);

    let output = wt_bin()
        .args(["rm", "unmerged-force", "--force", "--repo"])
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm --force should remove unmerged branch: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists());
    assert_branch_absent(&repo, "unmerged-force");
}

#[test]
fn removes_detached_head_worktree() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "detach-me");

    assert_git_success(&wt_path, &["checkout", "--detach"]);

    let output = wt_bin()
        .arg("rm")
        .arg(&wt_path)
        .arg("--repo")
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should remove detached HEAD worktree: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists(), "worktree directory should be removed");
    assert_branch_present(&repo, "detach-me");
}

#[test]
fn refuses_branch_checked_out_in_another_worktree() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "shared-branch");

    assert_git_success(&repo, &["symbolic-ref", "HEAD", "refs/heads/shared-branch"]);

    let output = wt_bin()
        .arg("rm")
        .arg(&wt_path)
        .arg("--force")
        .arg("--repo")
        .arg(&repo)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt rm should refuse when branch is checked out elsewhere"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("checked out in another worktree"),
        "expected checked-out-elsewhere error, got: {stderr}",
    );
    assert!(wt_path.exists());
}
