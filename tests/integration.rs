use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        .args(["new", branch, "--repo"])
        .arg(repo)
        .env("HOME", home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wt new {branch} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
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

        let wt_path = wt_new(home.path(), &repo, "existing");
        assert_eq!(
            assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
            "existing"
        );
    }

    #[test]
    fn warns_and_creates_branch_with_unreachable_origin() {
        let (home, repo) = setup();

        assert_git_success(
            &repo,
            &["remote", "add", "origin", "https://0.0.0.0/nonexistent.git"],
        );

        let output = wt_bin()
            .args(["new", "offline-branch", "--repo"])
            .arg(&repo)
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "wt new should succeed with unreachable origin: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert!(!path.is_empty());
        assert!(PathBuf::from(path).exists());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("wt: warning: cannot fetch from 'origin':"),
            "expected fetch warning, got: {stderr}",
        );
        assert!(
            stderr.contains("remote branch state may be stale"),
            "expected stale-checks warning, got: {stderr}",
        );
    }

    #[test]
    fn existing_local_branch_skips_fetch_warning() {
        let (home, repo) = setup();

        assert_git_success(&repo, &["branch", "existing"]);
        assert_git_success(
            &repo,
            &["remote", "add", "origin", "https://0.0.0.0/nonexistent.git"],
        );

        let output = wt_bin()
            .args(["new", "existing", "--repo"])
            .arg(&repo)
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "wt new should use local branch when available: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("branch 'existing' exists, checking out"),
            "expected existing-branch message, got: {stderr}",
        );
        assert!(
            !stderr.contains("warning: cannot fetch from 'origin'"),
            "should not fetch/warn when local branch exists, got: {stderr}",
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

        let wt_path = wt_new(home.path(), &repo, "remote-only");
        assert_eq!(
            assert_git_stdout_success(&wt_path, &["branch", "--show-current"]).trim(),
            "remote-only"
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
            stderr.contains("invalid branch name: bad..name"),
            "expected invalid-name error, got: {stderr}",
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
    fn refuses_detached_head_worktree() {
        let (home, repo) = setup();
        let wt_path = wt_new(home.path(), &repo, "detach-me");

        assert_git_success(&wt_path, &["checkout", "--detach"]);

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
            "wt rm should refuse detached HEAD worktree"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("detached HEAD"),
            "expected detached HEAD error, got: {stderr}",
        );
        assert!(wt_path.exists());
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
