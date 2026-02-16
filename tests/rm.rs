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
fn preserves_managed_parent_when_cwd_is_inside_parent() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "cwd-parent-guard");
    let parent_dir = wt_path.parent().unwrap().to_path_buf();

    let output = wt_bin()
        .args(["rm", "cwd-parent-guard", "--force", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .current_dir(&parent_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should remove the worktree: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "worktree should still be removed when cwd is in parent"
    );
    assert!(
        parent_dir.exists(),
        "managed parent directory should be preserved when cwd is inside it"
    );
    assert_branch_absent(&repo, "cwd-parent-guard");
}

#[test]
fn preserves_user_files_in_managed_parent() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "managed-parent-data");
    let parent_dir = wt_path.parent().unwrap().to_path_buf();
    let user_note = parent_dir.join("keep.txt");
    let user_dir = parent_dir.join("keep-dir");
    std::fs::write(&user_note, "do not delete").unwrap();
    std::fs::create_dir(&user_dir).unwrap();

    let output = wt_bin()
        .args(["rm", "managed-parent-data", "--force", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should remove worktree: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists(), "worktree should be removed");
    assert!(
        parent_dir.exists(),
        "managed parent should be preserved when it contains user data"
    );
    assert!(
        user_note.exists(),
        "user file in parent should be preserved"
    );
    assert!(
        user_dir.exists(),
        "user directory in parent should be preserved"
    );
    assert_branch_absent(&repo, "managed-parent-data");
}

#[test]
fn preserves_unmanaged_parent_and_siblings_when_removing_by_path() {
    let (home, repo) = setup();
    let unmanaged_parent = home.path().join("external-worktrees");
    let unmanaged_wt = unmanaged_parent.join("remove-external");
    let sibling_file = unmanaged_parent.join("keep.txt");
    let sibling_dir = unmanaged_parent.join("keep-dir");
    std::fs::create_dir(&unmanaged_parent).unwrap();
    std::fs::write(&sibling_file, "keep").unwrap();
    std::fs::create_dir(&sibling_dir).unwrap();

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "-b", "remove-external"])
            .arg(&unmanaged_wt)
            .arg("main");
    });

    let output = wt_bin()
        .arg("rm")
        .arg(&unmanaged_wt)
        .arg("--force")
        .arg("--repo")
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should remove unmanaged worktree: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !unmanaged_wt.exists(),
        "target unmanaged worktree should be removed"
    );
    assert!(
        unmanaged_parent.exists(),
        "unmanaged parent should never be removed by wt rm cleanup"
    );
    assert!(
        sibling_file.exists(),
        "sibling file in unmanaged parent should be preserved"
    );
    assert!(
        sibling_dir.exists(),
        "sibling directory in unmanaged parent should be preserved"
    );
    assert_branch_absent(&repo, "remove-external");
}

#[test]
fn preserves_empty_unmanaged_parent_when_removing_by_path() {
    let (home, repo) = setup();
    let unmanaged_parent = home.path().join("external-empty-parent");
    let unmanaged_wt = unmanaged_parent.join("remove-external-empty");
    std::fs::create_dir(&unmanaged_parent).unwrap();

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "-b", "remove-external-empty"])
            .arg(&unmanaged_wt)
            .arg("main");
    });

    let output = wt_bin()
        .arg("rm")
        .arg(&unmanaged_wt)
        .arg("--force")
        .arg("--repo")
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt rm should remove unmanaged worktree: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !unmanaged_wt.exists(),
        "target unmanaged worktree should be removed"
    );
    assert!(
        unmanaged_parent.exists(),
        "empty unmanaged parent should never be removed by wt rm cleanup"
    );
    assert_branch_absent(&repo, "remove-external-empty");
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
