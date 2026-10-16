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
fn skips_non_symlink() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-skip");
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    std::fs::remove_file(wt_path.join(".env")).unwrap();
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
fn skips_wrong_symlink() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    std::fs::write(repo.join(".other"), "OTHER").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-wrong");
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    std::fs::remove_file(wt_path.join(".env")).unwrap();
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
fn skips_missing_file() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-unlink-miss");
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());
    std::fs::remove_file(wt_path.join(".env")).unwrap();

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
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

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

#[test]
fn removes_config_entry_without_linked_worktrees() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET").unwrap();
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    let output = wt_unlink(home.path(), &repo, &["./.env"]);
    assert!(output.status.success());

    let list = run_wt(home.path(), |cmd| {
        cmd.args(["link", "--list", "--repo"]).arg(&repo);
    });
    assert_stdout_empty(&list);
}

#[test]
fn removes_config_entry_saved_before_paths_were_normalized() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET").unwrap();
    let feat = wt_new(home.path(), &repo, "feat");
    std::os::unix::fs::symlink(repo.join(".env"), feat.join(".env")).unwrap();
    std::fs::write(
        home.path().join(".wt/config"),
        format!(
            "[links]\n\"{}\" = [\"./.env\"]\n",
            canonical(&repo).display()
        ),
    )
    .unwrap();

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    let list = run_wt(home.path(), |cmd| {
        cmd.args(["link", "--list", "--repo"]).arg(&repo);
    });
    assert_eq!(String::from_utf8_lossy(&list.stdout), "");
}

#[test]
fn rejects_file_that_was_never_linked() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-never-linked");
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    let output = wt_unlink(home.path(), &repo, &[".env", ".evn"]);
    assert_error(&output, 1, "cannot unlink .evn: not linked\n");
    assert!(
        wt_path.join(".env").symlink_metadata().is_ok(),
        "nothing should be unlinked when an argument is invalid"
    );
}

#[test]
fn removes_directories_created_for_nested_links() {
    let (home, repo) = setup();
    std::fs::create_dir_all(repo.join("config/local")).unwrap();
    std::fs::write(repo.join("config/local/app.toml"), "k = 1").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-nested");
    assert!(
        wt_link(home.path(), &repo, &["config/local/app.toml"])
            .status
            .success()
    );
    assert!(wt_path.join("config/local/app.toml").is_symlink());

    let output = wt_unlink(home.path(), &repo, &["config/local/app.toml"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "unlinked config/local/app.toml in 'feat-nested'\n"
    );
    assert!(
        !wt_path.join("config").exists(),
        "empty parents should be removed"
    );
    assert!(wt_path.exists());
}

#[test]
fn keeps_parent_directories_that_hold_other_files() {
    let (home, repo) = setup();
    std::fs::create_dir_all(repo.join("config")).unwrap();
    std::fs::write(repo.join("config/app.toml"), "k = 1").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-nested-keep");
    assert!(
        wt_link(home.path(), &repo, &["config/app.toml"])
            .status
            .success()
    );
    std::fs::write(wt_path.join("config/other.toml"), "mine").unwrap();

    assert!(
        wt_unlink(home.path(), &repo, &["config/app.toml"])
            .status
            .success()
    );
    assert!(wt_path.join("config/other.toml").exists());
}
