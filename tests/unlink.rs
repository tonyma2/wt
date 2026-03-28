use std::path::Path;

pub mod common;

use common::*;

fn wt_link(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
    run_wt(home, |cmd| {
        cmd.arg("link");
        cmd.args(files);
        cmd.args(["--repo"]).arg(repo);
    })
}

fn wt_unlink(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
    run_wt(home, |cmd| {
        cmd.arg("unlink");
        cmd.args(files);
        cmd.args(["--repo"]).arg(repo);
    })
}

fn wt_unlink_all(home: &Path, repo: &Path) -> std::process::Output {
    run_wt(home, |cmd| {
        cmd.arg("unlink");
        cmd.arg("--all");
        cmd.args(["--repo"]).arg(repo);
    })
}

fn unlink_force(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
    run_wt(home, |cmd| {
        cmd.arg("unlink");
        cmd.arg("--force");
        cmd.args(files);
        cmd.args(["--repo"]).arg(repo);
    })
}

fn create_symlink(source: &Path, dest: &Path) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, dest).unwrap();
    }
    #[cfg(windows)]
    {
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(source, dest).unwrap();
        } else {
            std::os::windows::fs::symlink_file(source, dest).unwrap();
        }
    }
}

#[test]
fn unlinks_symlinked_file() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink");

    let link_out = wt_link(home.path(), &repo, &[".env"]);
    assert!(link_out.status.success());
    assert!(
        wt_path
            .join(".env")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(
        output.status.success(),
        "wt unlink failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unlinked .env"),
        "expected 'unlinked' message, got: {stderr}",
    );
    assert!(!wt_path.join(".env").exists(), "symlink should be removed",);
}

#[test]
fn skips_non_symlink_without_force() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-skip");

    std::fs::write(wt_path.join(".env"), "LOCAL=xyz").unwrap();

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a symlink"),
        "expected 'not a symlink', got: {stderr}",
    );
    assert!(
        wt_path.join(".env").exists(),
        "regular file should not be removed",
    );
}

#[test]
fn skips_wrong_symlink_without_force() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    std::fs::write(repo.join(".other"), "OTHER").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-wrong");

    create_symlink(&repo.join(".other"), &wt_path.join(".env"));

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("points elsewhere"),
        "expected 'points elsewhere', got: {stderr}",
    );
    assert!(
        wt_path
            .join(".env")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "wrong symlink should not be removed",
    );
}

#[test]
fn force_removes_non_symlink() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-force");

    std::fs::write(wt_path.join(".env"), "LOCAL=xyz").unwrap();

    let output = unlink_force(home.path(), &repo, &[".env"]);
    assert!(
        output.status.success(),
        "wt unlink --force failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unlinked .env"),
        "expected 'unlinked' message, got: {stderr}",
    );
    assert!(
        !wt_path.join(".env").exists(),
        "file should be removed with --force",
    );
}

#[test]
fn force_removes_wrong_symlink() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    std::fs::write(repo.join(".other"), "OTHER").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-fwrong");

    create_symlink(&repo.join(".other"), &wt_path.join(".env"));

    let output = unlink_force(home.path(), &repo, &[".env"]);
    assert!(output.status.success());

    assert!(
        wt_path.join(".env").symlink_metadata().is_err(),
        "wrong symlink should be removed with --force",
    );
}

#[test]
fn force_refuses_directory() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-forcedir");

    let dest_dir = wt_path.join(".env");
    std::fs::create_dir(&dest_dir).unwrap();
    std::fs::write(dest_dir.join("old.txt"), "old").unwrap();

    let output = unlink_force(home.path(), &repo, &[".env"]);
    assert!(
        !output.status.success(),
        "wt unlink --force should fail on directory: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot remove .env"),
        "expected 'cannot remove' error, got: {stderr}",
    );
    assert!(
        stderr.contains("destination is a directory"),
        "expected 'destination is a directory', got: {stderr}",
    );
    assert!(dest_dir.is_dir(), "directory should not have been deleted");
}

#[test]
fn skips_missing_file() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let _wt_path = wt_new(home.path(), &repo, "feat-unlink-miss");

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unlinked"),
        "should silently skip missing file, got: {stderr}",
    );
}

#[test]
fn unlinks_from_multiple_worktrees() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt1 = wt_new(home.path(), &repo, "feat-unlink-a");
    let wt2 = wt_new(home.path(), &repo, "feat-unlink-b");

    let link_out = wt_link(home.path(), &repo, &[".env"]);
    assert!(link_out.status.success());

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(output.status.success());

    assert!(!wt1.join(".env").exists(), "symlink removed from wt1");
    assert!(!wt2.join(".env").exists(), "symlink removed from wt2");
}

#[test]
fn no_linked_worktrees() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no linked worktrees"),
        "expected 'no linked worktrees', got: {stderr}",
    );
}

#[test]
fn rejects_absolute_path() {
    let (home, repo) = setup();

    let output = wt_unlink(home.path(), &repo, &["/etc/passwd"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("path must be relative"),
        "expected 'path must be relative', got: {stderr}",
    );
}

#[test]
fn rejects_dotdot() {
    let (home, repo) = setup();

    let output = wt_unlink(home.path(), &repo, &["../secret"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("must not contain '..'"),
        "expected 'must not contain ..', got: {stderr}",
    );
}

#[test]
fn all_unlinks_files_from_config() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    std::fs::write(repo.join(".env.local"), "LOCAL=xyz").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-all-unlink");

    let link_out = wt_link(home.path(), &repo, &[".env", ".env.local"]);
    assert!(link_out.status.success());
    assert!(
        wt_path
            .join(".env")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        wt_path
            .join(".env.local")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );

    let output = wt_unlink_all(home.path(), &repo);
    assert!(
        output.status.success(),
        "wt unlink --all failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        !wt_path.join(".env").exists(),
        ".env symlink should be removed"
    );
    assert!(
        !wt_path.join(".env.local").exists(),
        ".env.local symlink should be removed"
    );
}

#[test]
fn all_no_linked_worktrees() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();

    let link_out = wt_link(home.path(), &repo, &[".env"]);
    assert!(link_out.status.success());

    let output = wt_unlink_all(home.path(), &repo);
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no linked worktrees"),
        "expected 'no linked worktrees', got: {stderr}",
    );
}

#[test]
fn all_with_empty_config() {
    let (home, repo) = setup();
    let _wt_path = wt_new(home.path(), &repo, "feat-all-empty");

    let output = wt_unlink_all(home.path(), &repo);
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no linked files in config"),
        "expected 'no linked files in config', got: {stderr}",
    );
}

#[test]
fn all_conflicts_with_files() {
    let (home, repo) = setup();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["unlink", ".env", "--all", "--repo"]).arg(&repo);
    });
    assert_exit_code(&output, 2);
}

#[test]
fn no_files_and_no_all_errors() {
    let (home, repo) = setup();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["unlink", "--repo"]).arg(&repo);
    });
    assert_exit_code(&output, 2);
}

#[test]
fn all_removes_config_entries() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let _wt_path = wt_new(home.path(), &repo, "feat-all-cfg");

    let link_out = wt_link(home.path(), &repo, &[".env"]);
    assert!(link_out.status.success());

    let config_path = home.path().join(".wt").join("config");
    let before = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        before.contains(".env"),
        "config should contain .env before unlink"
    );

    let output = wt_unlink_all(home.path(), &repo);
    assert!(
        output.status.success(),
        "wt unlink --all failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let after = std::fs::read_to_string(&config_path).unwrap_or_default();
    assert!(
        !after.contains(".env"),
        "config should not contain .env after unlink --all, got: {after}",
    );
}

#[test]
fn all_unlinks_from_multiple_worktrees() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt1 = wt_new(home.path(), &repo, "feat-all-multi-a");
    let wt2 = wt_new(home.path(), &repo, "feat-all-multi-b");

    let link_out = wt_link(home.path(), &repo, &[".env"]);
    assert!(link_out.status.success());
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

    let output = wt_unlink_all(home.path(), &repo);
    assert!(
        output.status.success(),
        "wt unlink --all failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(!wt1.join(".env").exists(), "symlink removed from wt1");
    assert!(!wt2.join(".env").exists(), "symlink removed from wt2");
}
