use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

fn wt_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wt"))
}

fn git(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    cmd
}

fn assert_git_success(dir: &Path, args: &[&str]) {
    let status = git(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git failed to start");
    assert!(status.success(), "git {:?} failed", args);
}

fn assert_git_success_with(dir: &Path, configure: impl FnOnce(&mut Command)) {
    let mut cmd = git(dir);
    configure(&mut cmd);
    let program = cmd.get_program().to_string_lossy().to_string();
    let args: Vec<String> = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
    let output = cmd.output().expect("git failed to start");
    assert!(
        output.status.success(),
        "git {} {:?} failed: {}",
        program,
        args,
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_git_stdout_success(dir: &Path, args: &[&str]) -> String {
    let output = git(dir).args(args).output().expect("git failed to start");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn assert_branch_absent(repo: &Path, branch: &str) {
    let output = git(repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .stdout(Stdio::null())
        .output()
        .expect("git failed to start");
    assert!(
        !output.status.success(),
        "branch '{branch}' should be deleted from {}",
        repo.display()
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "git show-ref returned unexpected status for '{}': {}",
        branch,
        repo.display()
    );
}

fn assert_branch_present(repo: &Path, branch: &str) {
    let output = git(repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .stdout(Stdio::null())
        .output()
        .expect("git failed to start");
    assert!(
        output.status.success(),
        "branch '{branch}' should exist in {}",
        repo.display()
    );
}

fn setup() -> (TempDir, PathBuf) {
    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);
    (home, repo)
}

fn init_repo(dir: &Path) {
    assert_git_success(dir, &["init", "-b", "main"]);
    assert_git_success(dir, &["config", "user.name", "Test"]);
    assert_git_success(dir, &["config", "user.email", "t@t"]);
    assert_git_success(dir, &["commit", "--allow-empty", "-m", "init"]);
}

fn init_bare_repo(path: &Path) {
    let status = Command::new("git")
        .args(["init", "--bare"])
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git failed to start");
    assert!(status.success(), "git init --bare failed");
}

fn wt_new(home: &Path, repo: &Path, branch: &str) -> PathBuf {
    let output = wt_bin()
        .args(["new", "-c", branch, "--repo"])
        .arg(repo)
        .env("HOME", home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new {branch} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    parse_wt_new_path(&output)
}

fn wt_checkout(home: &Path, repo: &Path, name: &str) -> PathBuf {
    let output = wt_bin()
        .args(["new", name, "--repo"])
        .arg(repo)
        .env("HOME", home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new {name} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    parse_wt_new_path(&output)
}

fn parse_wt_new_path(output: &Output) -> PathBuf {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "wt new should print exactly one path line, got: {stdout:?}"
    );
    let path_str = lines[0].trim();
    assert!(!path_str.is_empty(), "wt new should print a non-empty path");
    let path = PathBuf::from(path_str);
    assert!(
        path.exists(),
        "wt new path should exist: {}",
        path.display()
    );
    path
}

mod new {
    use super::*;

    #[test]
    fn creates_worktree() {
        let (home, repo) = setup();
        let wt_path = wt_new(home.path(), &repo, "test-branch");
        assert!(wt_path.exists());
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("warning"),
            "should produce no warnings, got: {stderr}",
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
        assert!(
            !output.status.success(),
            "wt new should fail outside a git repository"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not a git repository; use --repo or run inside one"),
            "expected repository guidance, got: {stderr}",
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
        assert!(
            !output.status.success(),
            "wt new should reject existing destination path"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("path already exists"),
            "expected existing-path error, got: {stderr}",
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
}

mod rm {
    use super::*;

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
    }

    #[test]
    fn refuses_dirty_without_force() {
        let (home, repo) = setup();
        let wt_path = wt_new(home.path(), &repo, "dirty-branch");

        std::fs::write(wt_path.join("uncommitted.txt"), "changes").unwrap();

        let output = wt_bin()
            .args(["rm", "dirty-branch", "--repo"])
            .arg(&repo)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "wt rm should refuse dirty worktree"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("use --force"),
            "should tell user about --force, got: {stderr}",
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
        assert!(
            !output.status.success(),
            "wt rm should refuse unmerged branch without remote"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("use --force"),
            "should tell user about --force, got: {stderr}",
        );
        assert!(
            stderr.contains("has unpushed commits"),
            "should explain unmerged/unpushed safety check, got: {stderr}",
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
        assert!(
            !output.status.success(),
            "wt rm should refuse when branch is not merged into HEAD"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("use --force"),
            "should tell user about --force, got: {stderr}",
        );
        assert!(
            stderr.contains("has unpushed commits"),
            "should explain unmerged/unpushed safety check, got: {stderr}",
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
        assert!(
            !output.status.success(),
            "wt rm should fail when one target fails"
        );
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
}

mod link {
    use super::*;

    fn wt_link(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
        let mut cmd = wt_bin();
        cmd.arg("link");
        cmd.args(files);
        cmd.args(["--repo"]).arg(repo);
        cmd.env("HOME", home);
        cmd.output().unwrap()
    }

    fn link_force(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
        let mut cmd = wt_bin();
        cmd.arg("link");
        cmd.arg("--force");
        cmd.args(files);
        cmd.args(["--repo"]).arg(repo);
        cmd.env("HOME", home);
        cmd.output().unwrap()
    }

    #[test]
    fn links_file_into_worktree() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-link");

        let output = wt_link(home.path(), &repo, &[".env"]);
        assert!(
            output.status.success(),
            "wt link failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );

        let link = wt_path.join(".env");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "SECRET=abc");

        let target = std::fs::read_link(&link).unwrap();
        assert!(
            target.is_absolute(),
            "symlink should be absolute, got: {target:?}"
        );
    }

    #[test]
    fn idempotent_skip() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-idem");

        let out1 = wt_link(home.path(), &repo, &[".env"]);
        assert!(out1.status.success());
        let err1 = String::from_utf8_lossy(&out1.stderr);
        assert!(
            err1.contains("wt: linked .env"),
            "first run should create link, got: {err1}",
        );

        let out2 = wt_link(home.path(), &repo, &[".env"]);
        assert!(out2.status.success());
        let err2 = String::from_utf8_lossy(&out2.stderr);
        assert!(
            !err2.contains("wt: linked .env"),
            "second run should not re-link, got: {err2}",
        );
        assert!(
            !err2.contains("wt: skipped"),
            "correct symlink should not warn, got: {err2}",
        );
        assert!(
            wt_path
                .join(".env")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn errors_on_missing_source() {
        let (home, repo) = setup();
        let _wt_path = wt_new(home.path(), &repo, "feat-missing");

        let output = wt_link(home.path(), &repo, &[".env"]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not found in primary worktree"),
            "expected 'not found' error, got: {stderr}",
        );
    }

    #[test]
    fn rejects_absolute_path() {
        let (home, repo) = setup();
        let _wt_path = wt_new(home.path(), &repo, "feat-abs");

        let output = wt_link(home.path(), &repo, &["/etc/passwd"]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("path must be relative"),
            "expected 'relative' error, got: {stderr}",
        );
    }

    #[test]
    fn rejects_dotdot() {
        let (home, repo) = setup();
        let _wt_path = wt_new(home.path(), &repo, "feat-dotdot");

        let output = wt_link(home.path(), &repo, &["../etc/passwd"]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("must not contain '..'"),
            "expected '..' error, got: {stderr}",
        );
    }

    #[test]
    fn creates_nested_parents() {
        let (home, repo) = setup();
        std::fs::create_dir(repo.join("config")).unwrap();
        std::fs::write(repo.join("config/.env"), "NESTED=1").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-nested");

        let output = wt_link(home.path(), &repo, &["config/.env"]);
        assert!(
            output.status.success(),
            "wt link failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );

        let link = wt_path.join("config/.env");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "NESTED=1");
    }

    #[test]
    fn links_into_multiple_worktrees() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        let wt1 = wt_new(home.path(), &repo, "feat-a");
        let wt2 = wt_new(home.path(), &repo, "feat-b");

        let output = wt_link(home.path(), &repo, &[".env"]);
        assert!(
            output.status.success(),
            "wt link failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );

        assert!(
            wt1.join(".env")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            wt2.join(".env")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn no_linked_worktrees() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();

        let output = wt_link(home.path(), &repo, &[".env"]);
        assert!(output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("no linked worktrees"),
            "expected 'no linked worktrees', got: {stderr}",
        );
    }

    #[test]
    fn warns_when_regular_file_exists() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-conflict");

        std::fs::write(wt_path.join(".env"), "LOCAL=xyz").unwrap();

        let output = wt_link(home.path(), &repo, &[".env"]);
        assert!(output.status.success(), "should still exit 0");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("wt: skipped .env"),
            "expected skip warning, got: {stderr}",
        );
        assert!(
            stderr.contains("already exists"),
            "expected 'already exists', got: {stderr}",
        );

        assert!(
            !wt_path
                .join(".env")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink(),
            "should not have replaced the regular file",
        );
        assert_eq!(
            std::fs::read_to_string(wt_path.join(".env")).unwrap(),
            "LOCAL=xyz",
        );
    }

    #[test]
    fn warns_when_wrong_symlink_exists() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        std::fs::write(repo.join(".other"), "OTHER").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-wronglink");

        #[cfg(unix)]
        std::os::unix::fs::symlink(repo.join(".other"), wt_path.join(".env")).unwrap();

        let output = wt_link(home.path(), &repo, &[".env"]);
        assert!(output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("wt: skipped .env"),
            "expected skip warning for wrong symlink, got: {stderr}",
        );
    }

    #[test]
    fn force_replaces_regular_file() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-force");

        std::fs::write(wt_path.join(".env"), "LOCAL=xyz").unwrap();

        let output = link_force(home.path(), &repo, &[".env"]);
        assert!(
            output.status.success(),
            "wt link --force failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("wt: linked .env"),
            "should report linking, got: {stderr}",
        );
        assert!(
            !stderr.contains("wt: skipped"),
            "should not warn with --force, got: {stderr}",
        );

        let link = wt_path.join(".env");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "SECRET=abc");
    }

    #[test]
    fn force_replaces_wrong_symlink() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        std::fs::write(repo.join(".other"), "OTHER").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-forcelink");

        #[cfg(unix)]
        std::os::unix::fs::symlink(repo.join(".other"), wt_path.join(".env")).unwrap();

        let output = link_force(home.path(), &repo, &[".env"]);
        assert!(
            output.status.success(),
            "wt link --force failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );

        let link = wt_path.join(".env");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        let target = std::fs::read_link(&link).unwrap();
        assert_eq!(
            std::fs::read_to_string(&link).unwrap(),
            "SECRET=abc",
            "should now point to primary .env, points to: {target:?}",
        );
    }

    #[test]
    fn force_skips_correct_symlink() {
        let (home, repo) = setup();
        std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
        let wt_path = wt_new(home.path(), &repo, "feat-forceidem");

        let out1 = wt_link(home.path(), &repo, &[".env"]);
        assert!(out1.status.success());

        let out2 = link_force(home.path(), &repo, &[".env"]);
        assert!(out2.status.success());
        let stderr = String::from_utf8_lossy(&out2.stderr);
        assert!(
            !stderr.contains("wt: linked"),
            "should skip correct symlink even with --force, got: {stderr}",
        );

        assert!(
            wt_path
                .join(".env")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

mod prune {
    use super::*;

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
        // No ~/.worktrees/ exists

        let output = wt_bin()
            .args(["prune"])
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "wt prune should succeed with no ~/.worktrees: {}",
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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
            stderr.contains("repo/merged-branch (merged)"),
            "should report merged branch removal, got: {stderr}",
        );
    }

    #[test]
    fn skips_squash_merged_worktree() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
            stderr.contains("repo/cwd-merged (merged, current directory)"),
            "should include branch name and reason, got: {stderr}",
        );
        assert!(
            stderr.contains("current directory"),
            "should report skipping due to current directory, got: {stderr}",
        );
    }

    #[test]
    fn gone_flag_prunes_upstream_gone_worktree() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
    fn gone_flag_skips_no_upstream_worktree() {
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
    fn gone_flag_skips_dirty_worktree() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
    fn gone_and_merged_reports_merged() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
            stderr.contains("(merged)"),
            "should report merged (not upstream gone) when both apply, got: {stderr}",
        );
        assert!(
            !stderr.contains("upstream gone"),
            "should not report upstream gone when merged, got: {stderr}",
        );
    }

    #[test]
    fn gone_and_merged_succeeds_when_head_is_elsewhere() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
    fn dry_run_gone_flag() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
    fn prunes_merged_worktree_when_head_is_elsewhere() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
    fn gone_flag_skips_locked_worktree() {
        let (home, repo) = setup();
        let origin = home.path().join("origin.git");
        init_bare_repo(&origin);

        assert_git_success_with(&repo, |cmd| {
            cmd.args(["remote", "add", "origin"]).arg(&origin);
        });
        assert_git_success(&repo, &["push", "-u", "origin", "main"]);
        assert_git_success(&repo, &["fetch", "--prune", "origin"]);

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
}
