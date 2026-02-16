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

fn link_force(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
    run_wt(home, |cmd| {
        cmd.arg("link");
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

    create_symlink(&repo.join(".other"), &wt_path.join(".env"));

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
fn force_replaces_directory_conflict() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-forcedir");

    let dest_dir = wt_path.join(".env");
    std::fs::create_dir(&dest_dir).unwrap();
    std::fs::write(dest_dir.join("old.txt"), "old").unwrap();

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

    let link = wt_path.join(".env");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "SECRET=abc");
    assert!(
        !link.join("old.txt").exists(),
        "old directory contents should be removed"
    );
}

#[test]
fn force_replaces_wrong_symlink() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    std::fs::write(repo.join(".other"), "OTHER").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-forcelink");

    create_symlink(&repo.join(".other"), &wt_path.join(".env"));

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
