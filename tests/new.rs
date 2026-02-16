use std::path::PathBuf;

use tempfile::TempDir;

pub mod common;

use common::*;

#[test]
fn creates_worktree() {
    let (home, repo) = setup();
    let output = wt_bin()
        .args(["new", "-c", "test-branch", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new -c should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert_stderr_exact(&output, "wt: creating branch 'test-branch'\n");

    let wt_path = parse_wt_new_path(&output);
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "test-branch"
    );
}

#[test]
fn checks_out_existing_branch() {
    let (home, repo) = setup();

    assert_git_success(&repo, &["branch", "existing"]);

    let wt_path = wt_checkout(home.path(), &repo, "existing");
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "existing"
    );
}

#[test]
fn succeeds_with_unreachable_origin() {
    let (home, repo) = setup();

    assert_git_success(
        &repo,
        &["remote", "add", "origin", "https://0.0.0.0/nonexistent.git"],
    );

    let output = wt_bin()
        .args(["new", "-c", "offline-branch", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new should succeed with unreachable origin: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert_stderr_exact(&output, "wt: creating branch 'offline-branch'\n");

    let wt_path = parse_wt_new_path(&output);
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "offline-branch"
    );
}

#[test]
fn base_succeeds_when_only_remote_branch_exists() {
    let (home, repo) = setup();
    let origin = home.path().join("origin.git");

    init_bare_repo(&origin);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);
    assert_git_success(&repo, &["branch", "remote-only"]);
    assert_git_success(&repo, &["push", "-u", "origin", "remote-only"]);
    assert_git_success(&repo, &["branch", "-D", "remote-only"]);

    let output = wt_bin()
        .args(["new", "-c", "remote-only", "main", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new -c <base> should succeed when only remote branch exists: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let wt_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "remote-only"
    );
}

#[test]
fn checks_out_remote_branch_when_local_missing() {
    let (home, repo) = setup();
    let origin = home.path().join("origin.git");

    init_bare_repo(&origin);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);
    assert_git_success(&repo, &["branch", "remote-only"]);
    assert_git_success(&repo, &["push", "-u", "origin", "remote-only"]);
    assert_git_success(&repo, &["branch", "-D", "remote-only"]);

    let wt_path = wt_checkout(home.path(), &repo, "remote-only");
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "remote-only"
    );
}

#[test]
fn checks_out_remote_tracking_ref_as_detached_head() {
    let (home, repo) = setup();
    let origin = home.path().join("origin.git");

    init_bare_repo(&origin);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);
    assert_branch_absent(&repo, "origin/main");

    let origin_main = assert_git_stdout_success(&repo, &["rev-parse", "origin/main"])
        .trim()
        .to_string();
    let wt_path = wt_checkout(home.path(), &repo, "origin/main");
    let branch = assert_git_stdout_success(&wt_path, &["branch", "--show-current"]);
    assert!(
        branch.trim().is_empty(),
        "origin/main checkout should be detached HEAD, got branch: {branch}"
    );
    let wt_head = assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    assert_eq!(
        wt_head, origin_main,
        "worktree HEAD should match origin/main tip"
    );
}

#[test]
fn rejects_invalid_branch_name() {
    let (home, repo) = setup();

    let output = wt_bin()
        .args(["new", "bad..name", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt new should reject invalid branch names"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bad..name"),
        "expected error mentioning invalid branch name, got: {stderr}",
    );
}

#[test]
fn fails_when_repo_path_is_not_a_git_repository() {
    let home = TempDir::new().unwrap();
    let not_repo = home.path().join("not-repo");
    std::fs::create_dir(&not_repo).unwrap();

    let output = wt_bin()
        .args(["new", "feature", "--repo"])
        .arg(&not_repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert_error(
        &output,
        1,
        "wt: not a git repository; use --repo or run inside one\n",
    );
}

#[test]
fn rejects_when_destination_path_already_exists() {
    let (home, repo) = setup();
    let dest = home
        .path()
        .join(".worktrees")
        .join("repo")
        .join("existing-path");
    std::fs::create_dir_all(&dest).unwrap();

    let output = wt_bin()
        .args(["new", "existing-path", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert_error(
        &output,
        1,
        &format!("wt: path already exists: {}\n", dest.display()),
    );
}

#[test]
fn creates_worktree_from_head() {
    let (home, repo) = setup();
    let origin = home.path().join("origin.git");

    init_bare_repo(&origin);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);

    // Advance local HEAD beyond origin/main
    assert_git_success(&repo, &["commit", "--allow-empty", "-m", "local-only"]);

    let head_tip = assert_git_stdout_success(&repo, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    let origin_tip = assert_git_stdout_success(&repo, &["rev-parse", "origin/main"])
        .trim()
        .to_string();
    assert_ne!(head_tip, origin_tip, "HEAD and origin/main should differ");

    let wt_path = wt_new(home.path(), &repo, "feat/from-head");
    let wt_tip = assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    assert_eq!(
        wt_tip, head_tip,
        "new branch should start from HEAD, not origin/main"
    );
}

#[test]
fn creates_worktree_from_base() {
    let (home, repo) = setup();

    assert_git_success(&repo, &["checkout", "-b", "develop"]);
    std::fs::write(repo.join("dev.txt"), "develop").unwrap();
    assert_git_success(&repo, &["add", "dev.txt"]);
    assert_git_success(&repo, &["commit", "-m", "develop commit"]);
    let develop_tip = assert_git_stdout_success(&repo, &["rev-parse", "develop"])
        .trim()
        .to_string();
    assert_git_success(&repo, &["checkout", "main"]);

    let output = wt_bin()
        .args(["new", "-c", "feat/x", "develop", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new -c <base> should succeed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let wt_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let wt_tip = assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    assert_eq!(
        wt_tip, develop_tip,
        "new branch should start from develop, not main"
    );
}

#[test]
fn rejects_create_with_existing_branch() {
    let (home, repo) = setup();

    assert_git_success(&repo, &["branch", "existing"]);

    let output = wt_bin()
        .args(["new", "-c", "existing", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt new -c should fail for existing branch"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already exists") && stderr.contains("wt new existing"),
        "expected create guidance, got: {stderr}",
    );
}

#[test]
fn checks_out_tag_as_detached_head() {
    let (home, repo) = setup();

    assert_git_success(&repo, &["tag", "v1.0"]);

    let output = wt_bin()
        .args(["new", "v1.0", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new should check out a tag: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("checking out 'v1.0'"),
        "expected checkout message, got: {stderr}",
    );
    assert!(
        !stderr.contains("branch 'v1.0' exists"),
        "should not call a tag a branch, got: {stderr}",
    );

    let wt_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let branch = assert_git_stdout_success(&wt_path, &["branch", "--show-current"]);
    assert!(
        branch.trim().is_empty(),
        "tag checkout should be detached HEAD, got branch: {branch}",
    );
}

#[test]
fn checks_out_rev_as_detached_head() {
    let (home, repo) = setup();

    let head = assert_git_stdout_success(&repo, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    let short = &head[..8];

    let output = wt_bin()
        .args(["new", short, "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new should check out a rev: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let wt_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let branch = assert_git_stdout_success(&wt_path, &["branch", "--show-current"]);
    assert!(
        branch.trim().is_empty(),
        "rev checkout should be detached HEAD, got branch: {branch}",
    );
    let wt_head = assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    assert_eq!(wt_head, head, "should resolve to the same commit");
}

#[test]
fn allows_local_branches_with_origin_prefix() {
    let (home, repo) = setup();
    let origin = home.path().join("origin.git");

    init_bare_repo(&origin);

    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);
    assert_git_success(&repo, &["branch", "origin/main"]);

    let wt_path = wt_checkout(home.path(), &repo, "origin/main");
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "origin/main"
    );
}

#[test]
fn rejects_base_without_create() {
    let (home, repo) = setup();

    let output = wt_bin()
        .args(["new", "feat/x", "main", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt new <name> <base> without -c should fail"
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "wt new <name> <base> without -c should be a usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--create"),
        "expected clap guidance about --create, got: {stderr}",
    );
}

#[test]
fn does_not_strip_non_remote_prefix() {
    let (home, repo) = setup();

    let wt_path = wt_new(home.path(), &repo, "feat/foo");
    assert!(
        wt_path.ends_with("feat/foo"),
        "directory should be 'feat/foo': {}",
        wt_path.display(),
    );
    let branch = assert_git_stdout_success(&wt_path, &["branch", "--show-current"]);
    assert_eq!(branch.trim(), "feat/foo");
}

#[test]
fn rejects_unresolvable_base() {
    let (home, repo) = setup();
    let dest = home.path().join(".worktrees").join("repo").join("feat/x");

    let output = wt_bin()
        .args(["new", "-c", "feat/x", "nonexistent", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt new -c with nonexistent base should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "should produce an error message on stderr"
    );
    assert!(
        !stderr.contains(dest.to_string_lossy().as_ref()),
        "stderr should not repeat destination path, got: {stderr}"
    );
}

#[test]
fn checkout_missing_name_fails_without_creation() {
    let (home, repo) = setup();
    let dest = home.path().join(".worktrees").join("repo").join("missing");

    let output = wt_bin()
        .args(["new", "missing", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt new should fail for unresolved checkout names"
    );
    assert!(
        !dest.exists(),
        "wt new should not create destination on failure"
    );
    assert_branch_absent(&repo, "missing");
}

#[test]
fn checkout_error_does_not_fallback_to_creation() {
    let (home, repo) = setup();

    assert_git_success(&repo, &["branch", "existing"]);
    let existing_wt = home.path().join("existing-manual");
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add"])
            .arg(&existing_wt)
            .arg("existing");
    });

    let dest = home.path().join(".worktrees").join("repo").join("existing");

    let output = wt_bin()
        .args(["new", "existing", "--repo"])
        .arg(&repo)
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "wt new should surface checkout errors instead of creating a branch"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already used by worktree"),
        "expected checkout error from git, got: {stderr}",
    );
    assert!(
        !stderr.contains(dest.to_string_lossy().as_ref()),
        "stderr should not repeat destination path, got: {stderr}"
    );
    assert!(
        !dest.exists(),
        "wt new should not create destination on checkout failure"
    );
}
