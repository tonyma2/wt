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

fn assert_copy(path: &Path, content: &str) {
    let meta = path.symlink_metadata().unwrap();
    assert!(!meta.is_symlink(), "{} should be a copy", path.display());
    assert_eq!(std::fs::read_to_string(path).unwrap(), content);
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
    assert_copy(&wt_path.join(".env"), "SECRET=abc");
    assert_copy(&repo.join(".env"), "SECRET=abc");
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
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "unlinked .env in 'main', kept a copy\n"
    );
    assert!(wt_path.join(".env").symlink_metadata().is_err());
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

    assert_copy(&wt1.join(".env"), "SECRET=abc");
    assert_copy(&wt2.join(".env"), "SECRET=abc");
}

#[test]
fn restores_copy_when_only_one_worktree() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());
    assert!(repo.join(".env").is_symlink());

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    assert_copy(&repo.join(".env"), "SECRET=abc");
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

    assert_copy(&wt_path.join(".env"), "SECRET=abc");
    assert_copy(&wt_path.join(".env.local"), "LOCAL=xyz");
}

#[test]
fn all_with_empty_config() {
    let (home, repo) = setup();
    let _wt_path = wt_new(home.path(), &repo, "feat-all-empty");

    let output = wt_unlink_all(home.path(), &repo);
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no linked files"),
        "expected 'no linked files', got: {stderr}",
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
fn all_empties_link_list() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let _wt_path = wt_new(home.path(), &repo, "feat-all-cfg");
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    let output = wt_unlink_all(home.path(), &repo);
    assert!(
        output.status.success(),
        "wt unlink --all failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let list = run_wt(home.path(), |cmd| {
        cmd.args(["link", "--list", "--repo"]).arg(&repo);
    });
    assert_stdout_empty(&list);
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

    assert_copy(&wt1.join(".env"), "SECRET=abc");
    assert_copy(&wt2.join(".env"), "SECRET=abc");
}

#[test]
fn unlink_with_one_worktree_empties_link_list() {
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
fn unlinks_files_migrated_from_legacy_config() {
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
fn restores_nested_file_as_copy() {
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
        "unlinked config/local/app.toml in 'main', kept a copy\n\
         unlinked config/local/app.toml in 'feat-nested', kept a copy\n"
    );
    assert_copy(&wt_path.join("config/local/app.toml"), "k = 1");
    assert_copy(&repo.join("config/local/app.toml"), "k = 1");
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

#[test]
fn keeps_stored_file_when_no_worktree_links_to_it() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());
    std::fs::remove_file(repo.join(".env")).unwrap();

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert_exit_code(&output, 1);
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("cannot unlink .env: no worktree links to it, the only copy is ~/repo/.git/wt/links/.env"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(repo.join(".git/wt/links/.env")).unwrap(),
        "SECRET=abc"
    );
}

#[test]
fn restored_copies_keep_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (home, repo) = setup();
    let env = repo.join(".env");
    std::fs::write(&env, "SECRET=abc").unwrap();
    std::fs::set_permissions(&env, std::fs::Permissions::from_mode(0o600)).unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-mode");
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    assert!(wt_unlink(home.path(), &repo, &[".env"]).status.success());
    for path in [&env, &wt_path.join(".env")] {
        let mode = std::fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{}", path.display());
    }
}

#[test]
fn unlinks_a_path_given_twice_once() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    let output = wt_unlink(home.path(), &repo, &[".env", "./.env"]);
    assert!(output.status.success());
    assert_stderr_exact(&output, "unlinked .env in 'main', kept a copy\n");
    assert_copy(&repo.join(".env"), "SECRET=abc");
}

#[test]
fn removes_dangling_links_when_shared_copy_is_missing() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat");
    assert_exit_code(&wt_link(home.path(), &repo, &[".env"]), 0);
    std::fs::remove_file(repo.join(".git/wt/links/.env")).unwrap();

    let output = wt_unlink(home.path(), &repo, &[".env"]);
    assert_exit_code(&output, 0);
    assert!(repo.join(".env").symlink_metadata().is_err());
    assert!(wt_path.join(".env").symlink_metadata().is_err());
}
