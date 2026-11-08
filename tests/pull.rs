use std::path::Path;
use std::process::Output;

pub mod common;

use common::*;

fn pull(home: &Path) -> Output {
    run_wt(home, |cmd| {
        cmd.arg("pull").current_dir(home);
    })
}

fn push_from_clone(home: &Path, origin: &Path, branch: &str, commits: usize) {
    let other = home.join("other");
    if !other.exists() {
        assert_git_success_with(home, |cmd| {
            cmd.args(["clone", "--quiet"]).arg(origin).arg(&other);
        });
        assert_git_success(&other, &["config", "user.name", "Other"]);
        assert_git_success(&other, &["config", "user.email", "o@o"]);
    }
    assert_git_success(&other, &["fetch", "--quiet", "origin"]);
    assert_git_success(
        &other,
        &[
            "checkout",
            "--quiet",
            "-B",
            branch,
            &format!("origin/{branch}"),
        ],
    );
    for i in 0..commits {
        assert_git_success(
            &other,
            &["commit", "--allow-empty", "-m", &format!("remote {i}")],
        );
    }
    assert_git_success(&other, &["push", "--quiet", "origin", branch]);
}

fn pushed_worktree(home: &Path, repo: &Path, branch: &str) -> std::path::PathBuf {
    let wt = wt_new(home, repo, branch);
    assert_git_success(&wt, &["push", "--quiet", "-u", "origin", branch]);
    wt
}

fn head(dir: &Path) -> String {
    assert_git_stdout_success(dir, &["rev-parse", "HEAD"])
}

#[test]
fn fast_forwards_clean_worktrees_that_are_behind() {
    let (home, repo, origin) = setup_with_origin();
    let feat = pushed_worktree(home.path(), &repo, "feat");
    push_from_clone(home.path(), &origin, "main", 2);
    push_from_clone(home.path(), &origin, "feat", 1);

    let output = pull(home.path());
    assert_exit_code(&output, 0);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let ours: Vec<&str> = stderr
        .lines()
        .filter(|l| l.ends_with(':') || (l.starts_with("  ") && !l.starts_with("   ")))
        .collect();
    assert_eq!(
        ours,
        [
            tilde(home.path(), &repo) + ":",
            "  fetching from 'origin'".into(),
            "  updated main (2 commits)".into(),
            "  updated feat (1 commit)".into(),
        ],
        "{stderr}"
    );
    let other = home.path().join("other");
    assert_eq!(
        head(&repo),
        assert_git_stdout_success(&other, &["rev-parse", "origin/main"])
    );
    assert_eq!(
        head(&feat),
        assert_git_stdout_success(&other, &["rev-parse", "origin/feat"])
    );
}

#[test]
fn reports_up_to_date_repo() {
    let (home, repo, _origin) = setup_with_origin();
    pushed_worktree(home.path(), &repo, "feat");

    let output = pull(home.path());
    assert_exit_code(&output, 0);
    assert_stderr_exact(
        &output,
        &format!(
            "{}:\n  fetching from 'origin'\n  up to date\n",
            tilde(home.path(), &repo)
        ),
    );
}

#[test]
fn skips_dirty_diverged_and_gone_without_touching_them() {
    let (home, repo, origin) = setup_with_origin();
    let dirty = pushed_worktree(home.path(), &repo, "wip");
    let diverged = pushed_worktree(home.path(), &repo, "diverged");
    let gone = pushed_worktree(home.path(), &repo, "gone");
    wt_new(home.path(), &repo, "local-only");

    push_from_clone(home.path(), &origin, "wip", 1);
    push_from_clone(home.path(), &origin, "diverged", 1);
    std::fs::write(dirty.join("wip.txt"), "wip").unwrap();
    assert_git_success(&diverged, &["commit", "--allow-empty", "-m", "local"]);
    assert_git_success(&repo, &["push", "--quiet", "origin", ":gone"]);
    let before = [head(&dirty), head(&diverged), head(&gone)];

    let output = pull(home.path());
    assert_exit_code(&output, 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("  skipping wip (dirty)\n"), "{stderr}");
    assert!(
        stderr.contains("  skipping diverged (diverged)\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("  skipping gone (upstream gone)\n"),
        "{stderr}"
    );
    assert!(!stderr.contains("local-only"), "{stderr}");
    assert!(!stderr.contains("up to date"), "{stderr}");
    assert_eq!([head(&dirty), head(&diverged), head(&gone)], before);
    assert!(dirty.join("wip.txt").exists());
}

#[test]
fn dirty_worktree_with_nothing_to_pull_is_up_to_date() {
    let (home, repo, _origin) = setup_with_origin();
    let feat = pushed_worktree(home.path(), &repo, "feat");
    std::fs::write(feat.join("wip.txt"), "wip").unwrap();

    let output = pull(home.path());
    assert_exit_code(&output, 0);
    assert_stderr_exact(
        &output,
        &format!(
            "{}:\n  fetching from 'origin'\n  up to date\n",
            tilde(home.path(), &repo)
        ),
    );
}

#[test]
fn fetch_failure_skips_the_repo_and_fails() {
    let (home, repo, _origin) = setup_with_origin();
    pushed_worktree(home.path(), &repo, "feat");
    assert_git_success(
        &repo,
        &["remote", "set-url", "origin", "/nonexistent/origin.git"],
    );
    let before = head(&repo);

    let output = pull(home.path());
    assert_exit_code(&output, 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("  cannot fetch from 'origin'\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("  skipping main (fetch failed)\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("  skipping feat (fetch failed)\n"),
        "{stderr}"
    );
    assert!(stderr.ends_with("cannot pull 1 repo\n"), "{stderr}");
    assert_eq!(head(&repo), before);
}

#[test]
fn fetches_each_tracked_remote() {
    let (home, repo, origin) = setup_with_origin();
    let upstream = home.path().join("upstream.git");
    init_bare_repo(&upstream);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "upstream"]).arg(&upstream);
    });
    assert_git_success(&repo, &["push", "--quiet", "upstream", "main:fork"]);
    assert_git_success(&repo, &["fetch", "--quiet", "upstream"]);
    let fork = wt_checkout(home.path(), &repo, "fork");
    assert_git_success(&fork, &["branch", "--quiet", "-u", "upstream/fork"]);
    push_from_clone(home.path(), &upstream, "fork", 1);
    let _ = origin;

    let output = pull(home.path());
    assert_exit_code(&output, 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("  fetching from 'origin'\n"), "{stderr}");
    assert!(stderr.contains("  fetching from 'upstream'\n"), "{stderr}");
    assert!(stderr.contains("  updated fork (1 commit)\n"), "{stderr}");
}

#[test]
fn nothing_to_pull_prints_nothing() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat");

    let output = pull(home.path());
    assert_exit_code(&output, 0);
    assert_stdout_empty(&output);
    assert_stderr_empty(&output);
}
