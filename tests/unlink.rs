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
        stderr.contains("wt: unlinked .env"),
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
        stderr.contains("wt: unlinked .env"),
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
