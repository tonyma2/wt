use tempfile::TempDir;

pub mod common;

use common::*;

#[test]
fn prunes_stale_metadata_across_repos() {
    let home = TempDir::new().unwrap();

    let repo_a = home.path().join("repo-a");
    std::fs::create_dir(&repo_a).unwrap();
    init_repo(&repo_a);

    let repo_b = home.path().join("repo-b");
    std::fs::create_dir(&repo_b).unwrap();
    init_repo(&repo_b);

    // Create two worktrees per repo so discovery works after removing one
    let wt_a1 = wt_new(home.path(), &repo_a, "branch-a1");
    let _wt_a2 = wt_new(home.path(), &repo_a, "branch-a2");
    let wt_b1 = wt_new(home.path(), &repo_b, "branch-b1");
    let _wt_b2 = wt_new(home.path(), &repo_b, "branch-b2");

    // Manually remove one worktree dir per repo to create stale metadata
    std::fs::remove_dir_all(&wt_a1).unwrap();
    std::fs::remove_dir_all(&wt_b1).unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    // Verify git metadata was cleaned: re-adding the same branch should work
    let _wt_a1_new = wt_checkout(home.path(), &repo_a, "branch-a1");
    let _wt_b1_new = wt_checkout(home.path(), &repo_b, "branch-b1");
}

#[test]
fn repo_flag_scopes_to_single_repo() {
    let home = TempDir::new().unwrap();

    let repo_a = home.path().join("repo-a");
    std::fs::create_dir(&repo_a).unwrap();
    init_repo(&repo_a);

    let repo_b = home.path().join("repo-b");
    std::fs::create_dir(&repo_b).unwrap();
    init_repo(&repo_b);

    let wt_a = wt_new(home.path(), &repo_a, "branch-a");
    let wt_b = wt_new(home.path(), &repo_b, "branch-b");

    std::fs::remove_dir_all(&wt_a).unwrap();
    std::fs::remove_dir_all(&wt_b).unwrap();

    let output = wt_bin()
        .args(["prune", "--repo"])
        .arg(&repo_a)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --repo should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    // repo_a was pruned, so re-adding branch-a should work
    let _wt_a2 = wt_checkout(home.path(), &repo_a, "branch-a");

    // repo_b was NOT pruned, so its stale metadata blocks re-adding
    let output = wt_bin()
        .args(["new", "branch-b", "--repo"])
        .arg(&repo_b)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "branch-b should still be blocked by stale metadata in repo-b"
    );
}

#[test]
fn dry_run_does_not_remove() {
    let home = TempDir::new().unwrap();

    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let wt_path = wt_new(home.path(), &repo, "branch-dry");
    let _wt_keep = wt_new(home.path(), &repo, "branch-keep");

    std::fs::remove_dir_all(&wt_path).unwrap();

    let output = wt_bin()
        .args(["prune", "--dry-run"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    // Stale metadata should still exist since we only did dry-run
    // Trying to re-add should fail
    let output = wt_bin()
        .args(["new", "branch-dry", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "branch-dry should still be blocked by stale metadata after dry-run"
    );
}

#[test]
fn removes_orphaned_worktree_directories() {
    let home = TempDir::new().unwrap();

    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let wt_path = wt_new(home.path(), &repo, "orphan-branch");

    // Delete the backing repo entirely, making the worktree an orphan
    std::fs::remove_dir_all(&repo).unwrap();

    assert!(wt_path.exists(), "worktree dir should exist before prune");

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        !wt_path.exists(),
        "orphaned worktree directory should be removed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("removed"),
        "should report removal, got: {stderr}",
    );
}

#[test]
fn preserves_parent_dir_when_cwd_is_inside_orphan_parent() {
    let home = TempDir::new().unwrap();

    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let wt_path = wt_new(home.path(), &repo, "orphan-cwd");
    let parent_dir = wt_path.parent().unwrap().to_path_buf();

    // Delete the backing repo so the worktree becomes an orphan
    std::fs::remove_dir_all(&repo).unwrap();

    assert!(parent_dir.exists(), "parent dir should exist before prune");

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .current_dir(&parent_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "orphaned worktree should still be removed"
    );
    assert!(
        parent_dir.exists(),
        "parent directory should be preserved when cwd is inside it"
    );
}

#[test]
fn no_worktrees_dir_succeeds_silently() {
    let home = TempDir::new().unwrap();
    // No ~/.wt/worktrees/ exists

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed with no ~/.wt/worktrees: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(output.stdout.is_empty(), "should produce no stdout");
    assert!(
        output.stderr.is_empty(),
        "should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn prunes_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "merged-branch");

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "merged-branch"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "merged worktree directory should be removed"
    );
    assert_branch_absent(&repo, "merged-branch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("merged-branch (merged)"),
        "should report merged branch removal, got: {stderr}",
    );
}

#[test]
fn preserves_unmanaged_parent_when_pruning_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();
    let unmanaged_parent = home.path().join("custom-parent");
    let unmanaged_wt = unmanaged_parent.join("merged-external");
    std::fs::create_dir(&unmanaged_parent).unwrap();

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "-b", "merged-external"])
            .arg(&unmanaged_wt)
            .arg("main");
    });

    std::fs::write(unmanaged_wt.join("feature.txt"), "work").unwrap();
    assert_git_success(&unmanaged_wt, &["add", "feature.txt"]);
    assert_git_success(&unmanaged_wt, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "merged-external"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --repo should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !unmanaged_wt.exists(),
        "merged unmanaged worktree should still be removed"
    );
    assert!(
        unmanaged_parent.exists(),
        "unmanaged parent directory should not be removed"
    );
    assert_branch_absent(&repo, "merged-external");
}

#[test]
fn preserves_unmanaged_siblings_when_pruning_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();
    let unmanaged_parent = home.path().join("custom-parent-with-siblings");
    let unmanaged_wt = unmanaged_parent.join("merged-external-siblings");
    let sibling_file = unmanaged_parent.join("keep.txt");
    let sibling_dir = unmanaged_parent.join("keep-dir");
    std::fs::create_dir(&unmanaged_parent).unwrap();
    std::fs::write(&sibling_file, "keep").unwrap();
    std::fs::create_dir(&sibling_dir).unwrap();

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "-b", "merged-external-siblings"])
            .arg(&unmanaged_wt)
            .arg("main");
    });

    std::fs::write(unmanaged_wt.join("feature.txt"), "work").unwrap();
    assert_git_success(&unmanaged_wt, &["add", "feature.txt"]);
    assert_git_success(&unmanaged_wt, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "merged-external-siblings"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --repo should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !unmanaged_wt.exists(),
        "merged unmanaged worktree should still be removed"
    );
    assert!(
        unmanaged_parent.exists(),
        "unmanaged parent should never be removed by prune merged cleanup"
    );
    assert!(
        sibling_file.exists(),
        "sibling file in unmanaged parent should be preserved"
    );
    assert!(
        sibling_dir.exists(),
        "sibling directory in unmanaged parent should be preserved"
    );
    assert_branch_absent(&repo, "merged-external-siblings");
}

#[test]
fn preserves_unmanaged_parent_when_pruning_in_default_mode() {
    let (home, repo, _origin) = setup_with_origin();
    let _managed_anchor = wt_new(home.path(), &repo, "discover-anchor");

    let unmanaged_parent = home.path().join("default-mode-unmanaged-parent");
    let unmanaged_wt = unmanaged_parent.join("default-mode-external");
    std::fs::create_dir(&unmanaged_parent).unwrap();

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "-b", "default-mode-external"])
            .arg(&unmanaged_wt)
            .arg("main");
    });

    std::fs::write(unmanaged_wt.join("feature.txt"), "work").unwrap();
    assert_git_success(&unmanaged_wt, &["add", "feature.txt"]);
    assert_git_success(&unmanaged_wt, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "default-mode-external"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !unmanaged_wt.exists(),
        "merged unmanaged worktree should still be removed"
    );
    assert!(
        unmanaged_parent.exists(),
        "unmanaged parent should not be removed in default prune mode"
    );
    assert_branch_absent(&repo, "default-mode-external");
}

#[test]
fn preserves_user_files_in_managed_parent_when_pruning_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();
    let wt_path = wt_new(home.path(), &repo, "merged-parent-data");
    let parent_dir = wt_path.parent().unwrap().to_path_buf();
    let user_note = parent_dir.join("keep.txt");
    let user_dir = parent_dir.join("keep-dir");
    std::fs::write(&user_note, "do not delete").unwrap();
    std::fs::create_dir(&user_dir).unwrap();

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "merged-parent-data"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists(), "merged worktree should be removed");
    assert!(
        parent_dir.exists(),
        "managed parent should be preserved when it contains user data"
    );
    assert!(
        user_note.exists(),
        "user file in managed parent should be preserved"
    );
    assert!(
        user_dir.exists(),
        "user directory in managed parent should be preserved"
    );
    assert_branch_absent(&repo, "merged-parent-data");
}

#[test]
fn preserves_managed_parent_when_cwd_is_inside_merged_parent() {
    let (home, repo, _origin) = setup_with_origin();
    let wt_path = wt_new(home.path(), &repo, "cwd-parent-merged");
    let parent_dir = wt_path.parent().unwrap().to_path_buf();

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "cwd-parent-merged"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .current_dir(&parent_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "merged worktree should still be removed when cwd is in parent"
    );
    assert!(
        parent_dir.exists(),
        "managed parent directory should be preserved when cwd is inside it"
    );
    assert_branch_absent(&repo, "cwd-parent-merged");
}

#[test]
fn skips_squash_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "squash-branch");
    std::fs::write(wt_path.join("feature.txt"), "squash work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "squash feature"]);

    assert_git_success(&repo, &["merge", "--squash", "squash-branch"]);
    assert_git_success(&repo, &["commit", "-m", "squash merge squash-branch"]);
    assert_git_success(&repo, &["push", "origin", "main"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "squash-merged worktree should not be removed (not a direct ancestor)"
    );
}

#[test]
fn skips_upstream_gone_unmerged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "gone-branch");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "gone-branch"]);

    assert_git_success(&repo, &["push", "origin", "--delete", "gone-branch"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "unmerged worktree should not be removed just because upstream is gone"
    );
}

#[test]
fn dry_run_skips_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "dry-merged");

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "dry-merged"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--dry-run"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --dry-run should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "dry-run should not remove merged worktree"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("would remove"),
        "should report what would be removed, got: {stderr}",
    );

    let branch_exists = git(&repo)
        .args(["show-ref", "--verify", "--quiet", "refs/heads/dry-merged"])
        .status()
        .unwrap()
        .success();
    assert!(branch_exists, "dry-run should not delete the branch");
}

#[test]
fn skips_dirty_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "dirty-merged");

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "dirty-merged"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);
    std::fs::write(wt_path.join("uncommitted.txt"), "dirty").unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "dirty merged worktree should not be removed"
    );
}

#[test]
fn skips_unmerged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "unmerged-branch");

    std::fs::write(wt_path.join("new.txt"), "change").unwrap();
    assert_git_success(&wt_path, &["add", "new.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "local change"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(wt_path.exists(), "unmerged worktree should not be removed");
}

#[test]
fn skips_cwd_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "cwd-merged");

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "cwd-merged"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "worktree should not be removed when cwd is inside it"
    );
    let branch_exists = git(&repo)
        .args(["show-ref", "--verify", "--quiet", "refs/heads/cwd-merged"])
        .status()
        .unwrap()
        .success();
    assert!(
        branch_exists,
        "branch should not be deleted when worktree is skipped"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cwd-merged (merged, current directory)"),
        "should include branch name and reason, got: {stderr}",
    );
}

#[test]
fn gone_prunes_upstream_gone_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "gone-branch");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "gone-branch"]);

    assert_git_success(&repo, &["push", "origin", "--delete", "gone-branch"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "upstream-gone worktree should be removed with --gone"
    );
    assert_branch_absent(&repo, "gone-branch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("(upstream gone)"),
        "should report upstream gone reason, got: {stderr}",
    );
}

#[test]
fn gone_fetches_non_origin_remote_before_classifying_gone() {
    let (home, repo) = setup();
    let upstream = home.path().join("upstream.git");
    init_bare_repo(&upstream);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "upstream"]).arg(&upstream);
    });
    assert_git_success(&repo, &["push", "-u", "upstream", "main"]);

    let wt_path = wt_new(home.path(), &repo, "upstream-live");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "upstream", "upstream-live"]);
    assert_git_success(
        &repo,
        &["update-ref", "-d", "refs/remotes/upstream/upstream-live"],
    );

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "worktree should not be removed when upstream still exists remotely"
    );
    assert_branch_present(&repo, "upstream-live");
    let tracking_exists = git(&repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            "refs/remotes/upstream/upstream-live",
        ])
        .status()
        .unwrap()
        .success();
    assert!(
        tracking_exists,
        "prune --gone should refresh non-origin tracking refs before classifying gone"
    );
}

#[test]
fn gone_fetch_failure_only_skips_that_remote() {
    let (home, repo, origin) = setup_with_origin();
    let upstream = home.path().join("upstream.git");
    init_bare_repo(&upstream);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "upstream"]).arg(&upstream);
    });
    assert_git_success(&repo, &["push", "-u", "upstream", "main"]);

    let wt_origin = wt_new(home.path(), &repo, "origin-gone");
    std::fs::write(wt_origin.join("origin.txt"), "work").unwrap();
    assert_git_success(&wt_origin, &["add", "origin.txt"]);
    assert_git_success(&wt_origin, &["commit", "-m", "origin work"]);
    assert_git_success(&wt_origin, &["push", "-u", "origin", "origin-gone"]);
    assert_git_success(&origin, &["branch", "-D", "origin-gone"]);

    let wt_upstream = wt_new(home.path(), &repo, "upstream-live");
    std::fs::write(wt_upstream.join("upstream.txt"), "work").unwrap();
    assert_git_success(&wt_upstream, &["add", "upstream.txt"]);
    assert_git_success(&wt_upstream, &["commit", "-m", "upstream work"]);
    assert_git_success(&wt_upstream, &["push", "-u", "upstream", "upstream-live"]);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "set-url", "upstream", "/nonexistent"]);
    });

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_origin.exists(),
        "origin-tracking upstream-gone worktree should still be removed"
    );
    assert!(
        wt_upstream.exists(),
        "upstream worktree should be preserved"
    );
    assert_branch_absent(&repo, "origin-gone");
    assert_branch_present(&repo, "upstream-live");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot fetch from 'upstream'"),
        "should report upstream fetch failure, got: {stderr}",
    );
}

#[test]
fn gone_skips_no_upstream_worktree() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "local-only");

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "local work"]);

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "local-only branch (never pushed) should not be removed with --gone"
    );
}

#[test]
fn gone_skips_dirty_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "dirty-gone");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "dirty-gone"]);

    assert_git_success(&repo, &["push", "origin", "--delete", "dirty-gone"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    std::fs::write(wt_path.join("uncommitted.txt"), "dirty").unwrap();

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "dirty upstream-gone worktree should not be removed"
    );
}

#[test]
fn gone_and_merged_reports_both() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "both-branch");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "both-branch"]);

    assert_git_success(&repo, &["merge", "both-branch"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["push", "origin", "--delete", "both-branch"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists(), "merged+gone worktree should be removed");
    assert_branch_absent(&repo, "both-branch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("(merged, upstream gone)"),
        "should report both merged and upstream gone when both apply, got: {stderr}",
    );
}

#[test]
fn gone_and_merged_succeeds_when_head_is_elsewhere() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "both-diverged-head");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "both-diverged-head"]);

    assert_git_success(&repo, &["merge", "both-diverged-head"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["push", "origin", "--delete", "both-diverged-head"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);
    assert_git_success(&repo, &["checkout", "-b", "side", "HEAD~1"]);

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed even when HEAD is elsewhere: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "merged+gone worktree should be removed when HEAD is elsewhere"
    );
    assert_branch_absent(&repo, "both-diverged-head");
}

#[test]
fn gone_dry_run_reports_without_removing() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "dry-gone");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "dry-gone"]);

    assert_git_success(&repo, &["push", "origin", "--delete", "dry-gone"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--dry-run", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --dry-run --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "dry-run should not remove upstream-gone worktree"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("would remove"),
        "should report what would be removed, got: {stderr}",
    );
    assert!(
        stderr.contains("upstream gone"),
        "should mention upstream gone reason, got: {stderr}",
    );
    let branch_exists = git(&repo)
        .args(["show-ref", "--verify", "--quiet", "refs/heads/dry-gone"])
        .status()
        .unwrap()
        .success();
    assert!(branch_exists, "dry-run should not delete the branch");
}

#[test]
fn gone_dry_run_does_not_fetch() {
    let (home, repo, origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "dry-no-fetch");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "dry-no-fetch"]);

    // Delete branch directly in the bare repo so the local remote-tracking
    // ref is NOT pruned. A real fetch --prune would remove it.
    assert_git_success(&origin, &["branch", "-D", "dry-no-fetch"]);

    let output = wt_bin()
        .args(["prune", "--dry-run", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --dry-run --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    // The remote-tracking ref should still exist because dry-run must not fetch
    let tracking_exists = git(&repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            "refs/remotes/origin/dry-no-fetch",
        ])
        .status()
        .unwrap()
        .success();
    assert!(
        tracking_exists,
        "dry-run --gone should not fetch (remote-tracking ref should still exist)"
    );
}

#[test]
fn gone_skips_when_fetch_fails() {
    let (home, repo, origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "fetch-fail");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "fetch-fail"]);

    // Delete remote branch directly in bare repo, then break the remote
    // URL so fetch will fail.
    assert_git_success(&origin, &["branch", "-D", "fetch-fail"]);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "set-url", "origin", "/nonexistent"]);
    });

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed even when fetch fails: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "worktree should not be removed when fetch fails"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("skipping upstream-gone pruning"),
        "should warn about skipping upstream-gone pruning, got: {stderr}",
    );
}

#[test]
fn prunes_merged_worktree_when_head_is_elsewhere() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "head-elsewhere-merged");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "head-elsewhere-merged"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    // Move HEAD to a branch that does not contain the merge commit
    assert_git_success(&repo, &["checkout", "-b", "side", "HEAD~1"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed even when HEAD is elsewhere: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "merged worktree should be removed even when HEAD is elsewhere"
    );
    assert_branch_absent(&repo, "head-elsewhere-merged");
}

#[test]
fn skips_merged_when_default_branch_unknown() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "local-merged-no-origin");

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "local-merged-no-origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "merged worktree should be kept when default branch cannot be determined"
    );
    let branch_exists = git(&repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/local-merged-no-origin",
        ])
        .status()
        .unwrap()
        .success();
    assert!(
        branch_exists,
        "branch should not be deleted when merged pruning is skipped"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot determine default branch"),
        "should explain why merged pruning was skipped, got: {stderr}",
    );
}

#[test]
fn skips_base_branch_in_linked_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    // Create a linked worktree on main, move primary to a side branch
    let _wt_path = wt_new(home.path(), &repo, "side-branch");
    assert_git_success(&repo, &["checkout", "-b", "other"]);
    assert_git_success(&repo, &["push", "-u", "origin", "other"]);

    // main is trivially an ancestor of origin/main, but must not be pruned
    let main_wt = wt_checkout(home.path(), &repo, "main");

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        main_wt.exists(),
        "base branch worktree should not be pruned"
    );
    let branch_exists = git(&repo)
        .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
        .status()
        .unwrap()
        .success();
    assert!(branch_exists, "base branch should not be deleted");
}

#[test]
fn repo_flag_prunes_merged() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "repo-merged");

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&repo, &["merge", "repo-merged"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --repo should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "merged worktree should be removed with --repo"
    );
    assert_branch_absent(&repo, "repo-merged");
}

#[test]
fn repo_flag_gone_prunes_upstream_gone() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "repo-gone-branch");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "repo-gone-branch"]);

    assert_git_success(&repo, &["push", "origin", "--delete", "repo-gone-branch"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune", "--gone", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone --repo should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "upstream-gone worktree should be removed with --repo --gone"
    );
    assert_branch_absent(&repo, "repo-gone-branch");
}

#[test]
fn gone_skips_locked_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "locked-gone");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "locked-gone"]);

    assert_git_success(&repo, &["push", "origin", "--delete", "locked-gone"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "lock"]).arg(&wt_path);
    });

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "locked upstream-gone worktree should not be removed"
    );
}

#[test]
fn gone_skips_when_tracking_remote_is_missing() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "missing-remote");

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["config", "branch.missing-remote.remote", "ghost"]);
    });
    assert_git_success_with(&repo, |cmd| {
        cmd.args([
            "config",
            "branch.missing-remote.merge",
            "refs/heads/missing-remote",
        ]);
    });

    let output = wt_bin()
        .args(["prune", "--gone"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune --gone should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        wt_path.exists(),
        "worktree should be preserved when tracking remote is missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("remote 'ghost' not found; skipping upstream-gone pruning"),
        "expected missing-remote warning, got: {stderr}",
    );
}

#[test]
fn reports_repo_prune_failures_with_aggregate_error() {
    let home = TempDir::new().unwrap();
    let broken_repo = home.path().join("broken-repo");
    std::fs::create_dir(&broken_repo).unwrap();

    let fake_wt = home.path().join(".wt/worktrees/aabb11/synthetic-repo");
    std::fs::create_dir_all(&fake_wt).unwrap();
    let fake_gitdir = broken_repo.join(".git/worktrees/fake");
    std::fs::write(
        fake_wt.join(".git"),
        format!("gitdir: {}\n", fake_gitdir.display()),
    )
    .unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "wt: cannot prune {}: cannot prune worktree metadata\n",
            broken_repo.display()
        )),
        "expected per-repo prune error, got: {stderr}",
    );
    assert!(
        stderr.contains("wt: cannot prune 1 repo(s)\n"),
        "expected aggregate prune error, got: {stderr}",
    );
}

#[test]
fn warns_and_skips_when_dot_git_file_is_malformed() {
    let home = TempDir::new().unwrap();
    let malformed = home.path().join(".wt/worktrees/ccdd22/bad-repo");
    std::fs::create_dir_all(&malformed).unwrap();
    std::fs::write(malformed.join(".git"), "not-a-gitdir-line\n").unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("wt: warning: cannot parse"),
        "expected malformed .git warning, got: {stderr}",
    );
    assert!(
        malformed.exists(),
        "malformed worktree directory should be skipped rather than removed"
    );
}
