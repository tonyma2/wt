pub mod common;

use common::*;

fn wt_switch(home: &std::path::Path, repo: &std::path::Path, name: &str) -> std::path::PathBuf {
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

#[test]
fn switch_returns_existing_worktree_path() {
    let (home, repo) = setup();
    let path = wt_new(home.path(), &repo, "feat/existing");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/existing", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_empty(&output);

    let switch_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        canonical(&std::path::PathBuf::from(&switch_path)),
        canonical(&path),
    );
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

    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = std::path::PathBuf::from(&path_str);
    assert!(path.exists(), "worktree path should exist: {path_str}");
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

    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = std::path::PathBuf::from(&path_str);
    assert!(path.exists(), "worktree path should exist: {path_str}");
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

    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = std::path::PathBuf::from(&path_str);
    assert!(path.exists(), "worktree path should exist: {path_str}");
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

    let second_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        canonical(&std::path::PathBuf::from(&second_path)),
        canonical(&first_path),
    );
}

#[test]
fn switch_errors_on_ambiguous_name() {
    let (home, repo) = setup();

    let wt1_path = wt_new(home.path(), &repo, "feat/ambig");

    // Manually force the second worktree onto the same branch by using
    // git worktree add with --force (bypasses the "already checked out" guard).
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ambiguous"),
        "expected 'ambiguous' in stderr, got: {stderr}",
    );

    // Cleanup: remove the manual worktree so temp dir cleanup doesn't fail
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

    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        std::path::Path::new(&path_str).exists(),
        "worktree path should exist",
    );
}

#[test]
fn switch_cleans_up_on_failure() {
    let (home, repo) = setup();

    let wt_dir = home.path().join(".wt").join("worktrees");
    let before: Vec<_> = if wt_dir.exists() {
        std::fs::read_dir(&wt_dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect()
    } else {
        vec![]
    };

    // Use an invalid refname that git will reject as a branch name
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "bad:name", "--repo"]).arg(&repo);
    });

    assert!(!output.status.success());

    let after: Vec<_> = if wt_dir.exists() {
        std::fs::read_dir(&wt_dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect()
    } else {
        vec![]
    };

    assert_eq!(
        before.len(),
        after.len(),
        "no new directories should remain after failed switch",
    );
}
