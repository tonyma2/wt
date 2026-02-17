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
fn switch_skips_prunable_worktree() {
    let (home, repo) = setup();
    let path = wt_new(home.path(), &repo, "feat/prunable");

    std::fs::remove_dir_all(&path).unwrap();
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/prunable", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(
        &output,
        "wt: pruning stale worktree metadata\nwt: checking out 'feat/prunable'\n",
    );

    let new_path = parse_wt_new_path(&output);
    assert!(new_path.exists());
    assert_ne!(
        canonical(&new_path),
        canonical(&path),
        "should create a new worktree, not return the stale path"
    );
}

#[test]
fn switch_prunes_stale_metadata_when_live_match_exists() {
    let (home, repo) = setup();

    // Create two worktrees for the same branch (--force bypasses the guard)
    let live_path = wt_new(home.path(), &repo, "feat/mixed");
    let stale_dir = home.path().join(".wt").join("worktrees").join("stale");
    std::fs::create_dir_all(&stale_dir).unwrap();
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "--force", "--quiet"])
            .arg(&stale_dir)
            .arg("feat/mixed");
    });

    // Delete the second one to make it prunable
    std::fs::remove_dir_all(&stale_dir).unwrap();

    // switch should return the live worktree and prune the stale entry
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/mixed", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "wt: pruning stale worktree metadata\n");

    let switch_path = parse_wt_new_path(&output);
    assert_eq!(canonical(&switch_path), canonical(&live_path));

    // After pruning, `wt path` should not see ambiguity
    let path_output = run_wt(home.path(), |cmd| {
        cmd.args(["path", "feat/mixed", "--repo"]).arg(&repo);
    });
    assert!(
        path_output.status.success(),
        "wt path should succeed after prune, got: {}",
        String::from_utf8_lossy(&path_output.stderr),
    );
}

#[test]
fn switch_rejects_tag() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["tag", "v1.0"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "v1.0", "--repo"]).arg(&repo);
    });

    assert_error(
        &output,
        1,
        "wt: 'v1.0' is not a branch; use `wt new v1.0` to check out a ref\n",
    );
}

#[test]
fn switch_rejects_sha() {
    let (home, repo) = setup();
    let sha = assert_git_stdout_success(&repo, &["rev-parse", "HEAD"]);
    let sha = sha.trim();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", sha, "--repo"]).arg(&repo);
    });

    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    assert_stderr_exact(
        &output,
        &format!("wt: '{sha}' is not a branch; use `wt new {sha}` to check out a ref\n"),
    );
}

#[test]
fn switch_rejects_head() {
    let (home, repo, _origin) = setup_with_origin();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "HEAD", "--repo"]).arg(&repo);
    });

    assert_error(
        &output,
        1,
        "wt: 'HEAD' is not a branch; use `wt new HEAD` to check out a ref\n",
    );
}

#[test]
fn switch_errors_on_multi_remote_branch() {
    let (home, repo, _origin) = setup_with_origin();

    let second = home.path().join("second.git");
    init_bare_repo(&second);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "second"]).arg(&second);
    });

    assert_git_success(&repo, &["branch", "feat/multi"]);
    assert_git_success(&repo, &["push", "origin", "feat/multi"]);
    assert_git_success(&repo, &["push", "second", "feat/multi"]);
    assert_git_success(&repo, &["branch", "-D", "feat/multi"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/multi", "--repo"]).arg(&repo);
    });

    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("multiple remotes"),
        "expected multi-remote error, got: {stderr}",
    );
    assert!(
        stderr.contains("origin") && stderr.contains("second"),
        "expected remote names in error, got: {stderr}",
    );
}

#[test]
fn switch_skips_locked_missing_worktree() {
    let (home, repo) = setup();

    let wt_dir = home.path().join(".wt").join("worktrees").join("locked-wt");
    std::fs::create_dir_all(&wt_dir).unwrap();
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "add", "--quiet", "-b", "feat/locked-gone"])
            .arg(&wt_dir);
    });
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["worktree", "lock"]).arg(&wt_dir);
    });

    // Delete the directory — git won't mark it prunable because it's locked
    std::fs::remove_dir_all(&wt_dir).unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/locked-gone", "--repo"])
            .arg(&repo);
    });

    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot create worktree"),
        "expected worktree creation error, got: {stderr}",
    );
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
