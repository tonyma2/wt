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
    assert_stderr_exact(&output, "checking out 'feat/checkout-me'\n");

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
    assert_stderr_exact(&output, "creating branch 'feat/brand-new'\n");

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
    assert_stderr_exact(
        &output,
        "creating branch 'feat/remote-only' tracking 'origin/feat/remote-only'\n",
    );

    let path = parse_wt_new_path(&output);
    assert!(
        path.exists(),
        "worktree path should exist: {}",
        path.display()
    );
}

#[test]
fn switch_checks_out_remote_branch_nested_slash() {
    let (home, repo, _origin) = setup_with_origin();

    assert_git_success(&repo, &["branch", "feat/scope/topic"]);
    assert_git_success(&repo, &["push", "origin", "feat/scope/topic"]);
    assert_git_success(&repo, &["branch", "-D", "feat/scope/topic"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/scope/topic", "--repo"])
            .arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(
        &output,
        "creating branch 'feat/scope/topic' tracking 'origin/feat/scope/topic'\n",
    );

    let path = parse_wt_new_path(&output);
    let upstream = assert_git_stdout_success(&path, &["rev-parse", "--abbrev-ref", "@{upstream}"]);
    assert_eq!(upstream.trim(), "origin/feat/scope/topic");
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
    assert_stderr_exact(&output, "creating branch 'feat/alias-test'\n");

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
        "pruning stale worktree metadata\nchecking out 'feat/prunable'\n",
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
fn switch_returns_live_match_when_stale_duplicate_exists() {
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

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/mixed", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_empty(&output);

    let switch_path = parse_wt_new_path(&output);
    assert_eq!(canonical(&switch_path), canonical(&live_path));
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
        "'v1.0' is not a branch, use `wt switch <branch> v1.0` to start a branch from it\n",
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
        &format!(
            "'{sha}' is not a branch, use `wt switch <branch> {sha}` to start a branch from it\n"
        ),
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
        "'HEAD' is not a branch, use `wt switch <branch> HEAD` to start a branch from it\n",
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

#[test]
fn switch_detects_typo_close_match() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat/login");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/logni", "--repo"]).arg(&repo);
    });

    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean 'feat/login'"),
        "expected typo suggestion, got: {stderr}",
    );
    assert!(
        stderr.contains("wt switch -c feat/logni"),
        "expected -c hint, got: {stderr}",
    );
}

#[test]
fn switch_creates_when_no_close_match() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat/login");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/something-completely-different", "--repo"])
            .arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(
        &output,
        "creating branch 'feat/something-completely-different'\n",
    );
}

#[test]
fn switch_create_flag_bypasses_fuzzy_check() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat/login");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "-c", "feat/logni", "--repo"])
            .arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "creating branch 'feat/logni'\n");
    let path = parse_wt_new_path(&output);
    assert!(path.exists());
    assert_branch_present(&repo, "feat/logni");
}

#[test]
fn no_cd_hint_when_stdout_not_tty() {
    let (home, repo) = setup();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/hint-test", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "creating branch 'feat/hint-test'\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("cd '"),
        "cd hint should not appear when stdout is not a TTY, got: {stderr}",
    );
}

#[test]
fn switch_create_flag_long_form() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat/login");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "--create", "feat/logni", "--repo"])
            .arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "creating branch 'feat/logni'\n");
}

#[test]
fn switch_no_fuzzy_check_for_existing_branch() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["branch", "feat/login"]);
    assert_git_success(&repo, &["branch", "feat/logni"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/logni", "--repo"]).arg(&repo);
    });

    assert!(output.status.success());
    assert_stderr_exact(&output, "checking out 'feat/logni'\n");
}

#[test]
fn suggests_remote_only_branch_for_typo() {
    let (home, repo, _origin) = setup_with_origin();
    assert_git_success(&repo, &["branch", "feature-login"]);
    assert_git_success(&repo, &["push", "origin", "feature-login"]);
    assert_git_success(&repo, &["branch", "-D", "feature-login"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feature-logn", "--repo"]).arg(&repo);
    });
    assert_exit_code(&output, 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("did you mean 'feature-login'?"));
}

#[test]
fn remote_branch_wins_over_same_named_tag() {
    let (home, repo, _origin) = setup_with_origin();
    assert_git_success(&repo, &["branch", "rel"]);
    assert_git_success(&repo, &["push", "origin", "rel"]);
    assert_git_success(&repo, &["branch", "-D", "rel"]);
    assert_git_success(&repo, &["commit", "--allow-empty", "-m", "later"]);
    assert_git_success(&repo, &["tag", "rel"]);

    let switch = || {
        run_wt(home.path(), |cmd| {
            cmd.args(["switch", "rel", "--repo"]).arg(&repo);
        })
    };
    let first = parse_wt_new_path(&switch());
    let branch = assert_git_stdout_success(&first, &["branch", "--show-current"]);
    assert_eq!(branch.trim(), "rel");
    assert_eq!(
        parse_wt_new_path(&switch()),
        first,
        "second switch should reuse the worktree"
    );
}

#[test]
fn multiple_remotes_hint_creates_tracking_branch() {
    let (home, repo, _origin) = setup_with_origin();
    let upstream = home.path().join("upstream.git");
    init_bare_repo(&upstream);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "upstream"]).arg(&upstream);
    });
    assert_git_success(&repo, &["branch", "both"]);
    assert_git_success(&repo, &["push", "origin", "both"]);
    assert_git_success(&repo, &["push", "upstream", "both"]);
    assert_git_success(&repo, &["branch", "-D", "both"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "both", "--repo"]).arg(&repo);
    });
    assert!(String::from_utf8_lossy(&output.stderr).contains("use `wt switch both <remote>/both`"));

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "both", "upstream/both", "--repo"])
            .arg(&repo);
    });
    let wt = parse_wt_new_path(&output);
    let upstream = assert_git_stdout_success(&wt, &["rev-parse", "--abbrev-ref", "@{upstream}"]);
    assert_eq!(upstream.trim(), "upstream/both");
}

#[test]
fn creates_branch_from_head_not_remote_default() {
    let (home, repo, _origin) = setup_with_origin();
    assert_git_success(&repo, &["commit", "--allow-empty", "-m", "local-only"]);
    let head = assert_git_stdout_success(&repo, &["rev-parse", "HEAD"]);

    let wt_path = wt_new(home.path(), &repo, "feat/from-head");
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"]),
        head
    );
}

#[test]
fn base_sets_start_point() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["checkout", "-b", "develop"]);
    assert_git_success(&repo, &["commit", "--allow-empty", "-m", "develop commit"]);
    let develop = assert_git_stdout_success(&repo, &["rev-parse", "develop"]);
    assert_git_success(&repo, &["checkout", "main"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/x", "develop", "--repo"])
            .arg(&repo);
    });
    assert_stderr_exact(&output, "creating branch 'feat/x'\n");
    let wt_path = parse_wt_new_path(&output);
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"]),
        develop
    );
}

#[test]
fn base_accepts_tag() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["tag", "v1.0"]);
    assert_git_success(&repo, &["commit", "--allow-empty", "-m", "after tag"]);
    let tagged = assert_git_stdout_success(&repo, &["rev-parse", "v1.0"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "hotfix", "v1.0", "--repo"]).arg(&repo);
    });
    let wt_path = parse_wt_new_path(&output);
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "hotfix"
    );
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["rev-parse", "HEAD"]),
        tagged
    );
}

#[test]
fn base_rejects_existing_branch() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["branch", "existing"]);
    let wt_root = home.path().join(".wt").join("worktrees");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "existing", "main", "--repo"])
            .arg(&repo);
    });
    assert_error(
        &output,
        1,
        "cannot create branch 'existing': already exists, use `wt switch existing` to go there\n",
    );
    assert_eq!(dir_entry_count(&wt_root), 0);
}

#[test]
fn base_rejects_unresolvable_start_point() {
    let (home, repo) = setup();
    let wt_root = home.path().join(".wt").join("worktrees");

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/x", "nonexistent", "--repo"])
            .arg(&repo);
    });
    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    assert_eq!(dir_entry_count(&wt_root), 0);
    assert_branch_absent(&repo, "feat/x");
}

#[test]
fn base_creates_fresh_branch_when_only_remote_branch_exists() {
    let (home, repo, _origin) = setup_with_origin();
    assert_git_success(&repo, &["branch", "remote-only"]);
    assert_git_success(&repo, &["push", "origin", "remote-only"]);
    assert_git_success(&repo, &["branch", "-D", "remote-only"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "remote-only", "main", "--repo"])
            .arg(&repo);
    });
    assert_stderr_exact(&output, "creating branch 'remote-only'\n");
    parse_wt_new_path(&output);
}

#[test]
fn branch_from_remote_base_does_not_track_it() {
    let (home, repo, _origin) = setup_with_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat", "origin/main", "--repo"])
            .arg(&repo);
    });
    let wt = parse_wt_new_path(&output);
    let upstream = git(&wt)
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .output()
        .unwrap();
    assert!(
        !upstream.status.success(),
        "feat must not track origin/main"
    );
}

#[test]
fn checks_out_local_branch_with_remote_prefix() {
    let (home, repo, _origin) = setup_with_origin();
    assert_git_success(&repo, &["branch", "origin/main"]);

    let wt_path = wt_checkout(home.path(), &repo, "origin/main");
    assert_eq!(
        assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
        "origin/main"
    );
}

#[test]
fn rejects_invalid_branch_name() {
    let (home, repo) = setup();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "bad..name", "--repo"]).arg(&repo);
    });
    assert_exit_code(&output, 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("bad..name"));
}

#[test]
fn rejects_names_that_look_like_options() {
    let (home, repo) = setup();
    for name_args in [&["--", "--detach"][..], &["-c", "--", "-f"][..]] {
        let output = run_wt(home.path(), |cmd| {
            cmd.args(["switch", "--repo"]).arg(&repo).args(name_args);
        });
        assert_exit_code(&output, 1);
    }
}

#[test]
fn fails_when_repo_path_is_not_a_git_repository() {
    let (home, _repo) = setup();
    let not_repo = home.path().join("not-repo");
    std::fs::create_dir(&not_repo).unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feature", "--repo"]).arg(&not_repo);
    });
    assert_error(
        &output,
        1,
        &format!("not a git repository: {}\n", not_repo.display()),
    );
}

#[test]
fn succeeds_with_unreachable_origin() {
    let (home, repo) = setup();
    assert_git_success(
        &repo,
        &["remote", "add", "origin", "https://0.0.0.0/nonexistent.git"],
    );

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "offline-branch", "--repo"]).arg(&repo);
    });
    assert_stderr_exact(&output, "creating branch 'offline-branch'\n");
    parse_wt_new_path(&output);
}

#[test]
fn names_worktree_dir_after_remote_repo() {
    let home = tempfile::TempDir::new().unwrap();
    let repo = home.path().join("local-name");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);
    assert_git_success(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://0.0.0.0/org/remote-name.git",
        ],
    );

    let wt_path = wt_new(home.path(), &repo, "feat/remote-test");
    assert!(wt_path.ends_with("remote-name"), "{}", wt_path.display());
}

#[test]
fn names_worktree_dir_after_local_dir_without_remote() {
    let home = tempfile::TempDir::new().unwrap();
    let repo = home.path().join("my-project");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let wt_path = wt_new(home.path(), &repo, "feat/no-remote");
    assert!(wt_path.ends_with("my-project"), "{}", wt_path.display());
}

#[test]
fn branch_slashes_do_not_affect_worktree_dir_name() {
    let (home, repo) = setup();

    let wt_path = wt_new(home.path(), &repo, "feat/foo");
    assert!(wt_path.ends_with("repo"), "{}", wt_path.display());
}

#[test]
fn accepts_bare_repo_as_repo_flag() {
    let (home, _repo, origin) = setup_with_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "-c", "from-bare", "--repo"])
            .arg(&origin);
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(parse_wt_new_path(&output).file_name().unwrap(), "origin");
}
