use std::path::Path;

use tempfile::TempDir;

pub mod common;

use common::*;

fn commit_file(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), name).unwrap();
    assert_git_success(dir, &["add", name]);
    assert_git_success(dir, &["commit", "-m", name]);
}

fn squash_merge(repo: &Path, branch: &str) {
    assert_git_success(repo, &["merge", "--squash", branch]);
    assert_git_success(repo, &["commit", "-m", &format!("squash {branch}")]);
    assert_git_success(repo, &["push", "origin", "HEAD:main"]);
}

#[test]
fn leaves_metadata_of_missing_worktrees_to_git() {
    let (home, repo) = setup();
    let missing = wt_new(home.path(), &repo, "missing");
    let _anchor = wt_new(home.path(), &repo, "anchor");
    std::fs::remove_dir_all(&missing).unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let listing = assert_git_stdout_success(&repo, &["worktree", "list", "--porcelain"]);
    assert!(listing.contains("prunable"), "{listing}");
    assert_branch_present(&repo, "missing");
}

#[test]
fn no_worktrees_dir_succeeds_silently() {
    let home = TempDir::new().unwrap();

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
    assert_git_success(&wt_path, &["push", "-u", "origin", "merged-branch"]);
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
fn preserves_unmanaged_siblings_when_pruning_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();
    let _managed_anchor = wt_new(home.path(), &repo, "discover-anchor");
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
    assert_git_success(
        &unmanaged_wt,
        &["push", "-u", "origin", "merged-external-siblings"],
    );
    assert_git_success(&repo, &["merge", "merged-external-siblings"]);
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
    assert_git_success(
        &unmanaged_wt,
        &["push", "-u", "origin", "default-mode-external"],
    );
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
    assert_git_success(&wt_path, &["push", "-u", "origin", "merged-parent-data"]);
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
    assert_git_success(&wt_path, &["push", "-u", "origin", "cwd-parent-merged"]);
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

#[cfg(unix)]
#[test]
fn removes_managed_parent_when_home_is_symlinked() {
    use std::os::unix::fs::symlink;

    let sandbox = TempDir::new().unwrap();
    let real_home = sandbox.path().join("real-home");
    std::fs::create_dir(&real_home).unwrap();

    let repo = real_home.join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let origin = real_home.join("origin.git");
    init_bare_repo(&origin);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let home_link = sandbox.path().join("home-link");
    symlink(&real_home, &home_link).unwrap();

    let wt_path = wt_new(&home_link, &repo, "symlink-home-prune");
    let parent_dir = wt_path.parent().unwrap().to_path_buf();

    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "symlink-home-prune"]);
    assert_git_success(&repo, &["merge", "symlink-home-prune"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", &home_link)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should remove merged worktree with symlinked HOME: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!wt_path.exists(), "merged worktree should be removed");
    assert!(
        !parent_dir.exists(),
        "managed parent should be removed when HOME resolves through a symlink"
    );
    assert_branch_absent(&repo, "symlink-home-prune");
}

#[test]
fn prunes_squash_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "squash-branch");
    commit_file(&wt_path, "feature.txt");
    assert_git_success(&wt_path, &["push", "-u", "origin", "squash-branch"]);
    squash_merge(&repo, "squash-branch");

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
        "squash-merged worktree should be removed even though its remote branch remains"
    );
    assert_branch_absent(&repo, "squash-branch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("removed squash-branch (merged)"),
        "got: {stderr}"
    );
}

#[test]
fn skips_unpushed_ancestor_branch() {
    let (home, repo, _origin) = setup_with_origin();
    let wt_path = wt_new(home.path(), &repo, "unpushed");

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
        "unpushed branch should not be pruned as merged"
    );
    assert_branch_present(&repo, "unpushed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("skipping unpushed (no upstream)"),
        "should report skip reason, got: {stderr}",
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
    assert_git_success(&wt_path, &["push", "-u", "origin", "dry-merged"]);
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
    assert_git_success(&wt_path, &["push", "-u", "origin", "dirty-merged"]);
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("dirty-merged (merged, dirty)"),
        "should report dirty skip reason, got: {stderr}",
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
    assert_git_success(&wt_path, &["push", "-u", "origin", "cwd-merged"]);
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
fn prunes_squash_merged_worktree_whose_upstream_is_gone() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "gone-branch");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "gone-branch"]);

    squash_merge(&repo, "gone-branch");
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
        !wt_path.exists(),
        "upstream-gone worktree should be removed"
    );
    assert_branch_absent(&repo, "gone-branch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("removed gone-branch (merged)"),
        "got: {stderr}",
    );
}

#[test]
fn fetch_failure_falls_back_to_local_refs() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "fetch-fail");
    commit_file(&wt_path, "feature.txt");
    assert_git_success(&wt_path, &["push", "-u", "origin", "fetch-fail"]);
    squash_merge(&repo, "fetch-fail");
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "set-url", "origin", "/nonexistent"]);
    });

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt prune should succeed even when fetch fails: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !wt_path.exists(),
        "a branch merged into the local copy of the base should still be removed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot fetch from 'origin', using local refs"),
        "got: {stderr}",
    );
}

#[test]
fn fetches_origin_before_checking_merged() {
    let (home, repo, origin) = setup_with_origin();
    let wt_path = wt_new(home.path(), &repo, "landed");
    commit_file(&wt_path, "feature.txt");
    assert_git_success(&wt_path, &["push", "-u", "origin", "landed"]);

    let other = home.path().join("other");
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["clone"]).arg(&origin).arg(&other);
    });
    assert_git_success(&other, &["config", "user.name", "Test"]);
    assert_git_success(&other, &["config", "user.email", "t@t"]);
    assert_git_success(&other, &["merge", "--squash", "origin/landed"]);
    assert_git_success(&other, &["commit", "-m", "squash landed"]);
    assert_git_success(&other, &["push", "origin", "main"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !wt_path.exists(),
        "merge seen only on the remote should count"
    );
    assert_branch_absent(&repo, "landed");
}

#[test]
fn prunes_merged_worktree_when_head_is_elsewhere() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "head-elsewhere-merged");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "add feature"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "head-elsewhere-merged"]);
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
fn reports_locked_merged_worktree() {
    let (home, repo, _origin) = setup_with_origin();

    let wt_path = wt_new(home.path(), &repo, "locked-merged");
    std::fs::write(wt_path.join("feature.txt"), "work").unwrap();
    assert_git_success(&wt_path, &["add", "feature.txt"]);
    assert_git_success(&wt_path, &["commit", "-m", "feature work"]);
    assert_git_success(&wt_path, &["push", "-u", "origin", "locked-merged"]);
    assert_git_success(&repo, &["merge", "locked-merged"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "lock"]).arg(&wt_path);
    });

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
        "locked merged worktree should not be removed"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("(merged, locked)"),
        "should report locked skip reason, got: {stderr}",
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
            "cannot prune {}: cannot list worktrees",
            broken_repo.display()
        )),
        "expected per-repo prune error, got: {stderr}",
    );
    assert!(
        stderr.contains("cannot prune 1 repo\n"),
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
        stderr.contains("cannot parse"),
        "expected malformed .git warning, got: {stderr}",
    );
    assert!(
        malformed.exists(),
        "malformed worktree directory should be skipped rather than removed"
    );
}

#[test]
fn multi_repo_output_is_grouped_by_repo() {
    let (home, repo_a, _origin_a) = setup_with_origin();

    let repo_b = home.path().join("repo-b");
    std::fs::create_dir(&repo_b).unwrap();
    init_repo(&repo_b);
    let origin_b = home.path().join("origin-b.git");
    init_bare_repo(&origin_b);
    assert_git_success_with(&repo_b, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin_b);
    });
    assert_git_success(&repo_b, &["push", "-u", "origin", "main"]);
    assert_git_success(&repo_b, &["fetch", "--prune", "origin"]);

    let wt_a = wt_new(home.path(), &repo_a, "merged-a");
    std::fs::write(wt_a.join("f.txt"), "a").unwrap();
    assert_git_success(&wt_a, &["add", "f.txt"]);
    assert_git_success(&wt_a, &["commit", "-m", "work"]);
    assert_git_success(&wt_a, &["push", "-u", "origin", "merged-a"]);
    assert_git_success(&repo_a, &["merge", "merged-a"]);
    assert_git_success(&repo_a, &["push", "origin", "main"]);
    assert_git_success(&repo_a, &["fetch", "--prune", "origin"]);

    let wt_b = wt_new(home.path(), &repo_b, "merged-b");
    std::fs::write(wt_b.join("f.txt"), "b").unwrap();
    assert_git_success(&wt_b, &["add", "f.txt"]);
    assert_git_success(&wt_b, &["commit", "-m", "work"]);
    assert_git_success(&wt_b, &["push", "-u", "origin", "merged-b"]);
    assert_git_success(&repo_b, &["merge", "merged-b"]);
    assert_git_success(&repo_b, &["push", "origin", "main"]);
    assert_git_success(&repo_b, &["fetch", "--prune", "origin"]);

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
    let repo_a_name = repo_a.file_name().unwrap().to_string_lossy();
    let repo_b_name = repo_b.file_name().unwrap().to_string_lossy();

    assert!(
        stderr.contains(&format!("{repo_a_name}:\n")),
        "should print repo-a header, got: {stderr}",
    );
    assert!(
        stderr.contains(&format!("{repo_b_name}:\n")),
        "should print repo-b header, got: {stderr}",
    );
    assert!(
        stderr.contains("  removed merged-a (merged)"),
        "repo-a messages should be indented, got: {stderr}",
    );
    assert!(
        stderr.contains("  removed merged-b (merged)"),
        "repo-b messages should be indented, got: {stderr}",
    );
}

#[test]
fn silent_repos_omitted_from_grouped_output() {
    let (home, repo, origin) = setup_with_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.arg("clone").arg(&origin);
    });
    assert!(output.status.success());

    let wt_path = wt_new(home.path(), &repo, "merged-only");
    commit_file(&wt_path, "f.txt");
    assert_git_success(&wt_path, &["push", "-u", "origin", "merged-only"]);
    assert_git_success(&repo, &["merge", "merged-only"]);
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

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("removed merged-only (merged)"), "{stderr}");
    assert!(
        !stderr.contains(".wt/repos/"),
        "a clone with only its default worktree has nothing to report, got: {stderr}",
    );
}

#[test]
fn leaves_a_renamed_worktree_repairable() {
    let (home, repo) = setup();
    let wt_path = wt_new(home.path(), &repo, "work");
    std::fs::write(wt_path.join("precious.txt"), "keep").unwrap();
    let renamed = home.path().join(".wt/worktrees/renamed");
    std::fs::rename(wt_path.parent().unwrap(), &renamed).unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "repair"]).arg(renamed.join("repo"));
    });
    assert_git_success(&renamed.join("repo"), &["status"]);
    assert_eq!(
        std::fs::read_to_string(renamed.join("repo/precious.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn never_prunes_default_branch_of_cloned_repo() {
    let (home, _repo, origin) = setup_with_origin();
    assert_git_success(&origin, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    let output = run_wt(home.path(), |cmd| {
        cmd.arg("clone").arg(&origin);
    });
    assert!(output.status.success());
    let main_wt = parse_wt_new_path(&output);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(main_wt.exists(), "default branch worktree must be kept");
    assert_git_success(&main_wt, &["rev-parse", "--verify", "refs/heads/main"]);
}

#[test]
fn keeps_branch_with_commits_after_squash_merge() {
    let (home, repo, origin) = setup_with_origin();
    let wt_path = wt_new(home.path(), &repo, "squashed");
    commit_file(&wt_path, "pushed.txt");
    assert_git_success(&wt_path, &["push", "-u", "origin", "squashed"]);
    squash_merge(&repo, "squashed");
    commit_file(&wt_path, "local-only.txt");
    assert_git_success(&origin, &["branch", "-D", "squashed"]);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(wt_path.exists());
    assert_branch_present(&repo, "squashed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("removed") && !stderr.contains("skipping"),
        "got: {stderr}"
    );
    assert!(
        stderr.contains(":\n  fetching from 'origin'\n"),
        "fetch note should be grouped under its repo, got: {stderr}"
    );
}

#[test]
fn keeps_fresh_branch_that_tracks_the_base() {
    let (home, repo, _origin) = setup_with_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "fresh", "origin/main"])
            .current_dir(&repo);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(wt_path.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("skipping fresh (no upstream)"),
        "got: {stderr}"
    );
}

#[test]
fn headers_distinguish_repos_with_the_same_name() {
    let home = TempDir::new().unwrap();
    for parent in ["a", "b"] {
        let repo = home.path().join(parent).join("proj");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        wt_new(home.path(), &repo, "never-pushed");
    }

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("~/a/proj:\n") && stderr.contains("~/b/proj:\n"),
        "got: {stderr}"
    );
}

#[test]
fn reports_orphans_without_removing_them() {
    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);
    let wt_path = wt_new(home.path(), &repo, "orphan-branch");
    std::fs::write(wt_path.join("uncommitted.txt"), "work").unwrap();
    std::fs::remove_dir_all(&repo).unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_stdout_empty(&output);
    assert!(wt_path.join("uncommitted.txt").exists());
    assert_stderr_exact(
        &output,
        &format!(
            "skipping {} (orphan, not registered with a repo, remove it manually)\n",
            tilde(home.path(), &wt_path)
        ),
    );
}

#[test]
fn prunes_merged_worktree_that_shared_files_without_losing_them() {
    let (home, repo, _origin) = setup_with_origin();
    std::fs::write(repo.join(".git/info/exclude"), ".env\n").unwrap();
    let feat = wt_new(home.path(), &repo, "feat");
    assert_git_success(&feat, &["push", "-u", "origin", "feat"]);
    std::fs::write(feat.join(".env"), "SECRET").unwrap();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["link", ".env"]).current_dir(&feat);
    });
    assert!(output.status.success());
    assert!(repo.join(".env").is_symlink());

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("removed feat (merged)"), "got: {stderr}");
    assert!(!feat.exists());
    assert_eq!(
        std::fs::read_to_string(repo.join(".env")).unwrap(),
        "SECRET"
    );
}

#[test]
fn keeps_merged_worktree_holding_another_worktree() {
    let (home, repo, _origin) = setup_with_origin();
    let outer = wt_new(home.path(), &repo, "outer");
    std::fs::write(outer.join(".gitignore"), "build/\n").unwrap();
    assert_git_success(&outer, &["add", ".gitignore"]);
    assert_git_success(&outer, &["commit", "-m", "ignore build"]);
    assert_git_success(&outer, &["push", "-u", "origin", "outer"]);
    assert_git_success(&repo, &["merge", "outer"]);
    assert_git_success(&repo, &["push", "origin", "main"]);
    let inner = outer.join("build/inner");
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "-b", "inner"]).arg(&inner);
    });
    std::fs::write(inner.join("wip.txt"), "wip").unwrap();

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(inner.join("wip.txt").exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("skipping outer (merged, contains a worktree)"),
        "got: {stderr}"
    );
}

#[test]
fn branch_tracking_another_branch_is_not_pushed() {
    let (home, repo, origin) = setup_with_origin();
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "upstream"]).arg(&origin);
    });
    assert_git_success(&repo, &["fetch", "upstream"]);
    let wt_path = home.path().join(".wt/worktrees/f00001/repo");
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "--track", "-b", "forkfeat"])
            .arg(&wt_path)
            .arg("upstream/main");
    });

    let output = wt_bin()
        .args(["prune"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(wt_path.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("skipping forkfeat (no upstream)"),
        "got: {stderr}"
    );
}
