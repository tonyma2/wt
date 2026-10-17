use std::path::Path;

pub mod common;

use common::*;

fn wt_link(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
    run_wt(home, |cmd| {
        cmd.arg("link");
        cmd.args(files);
        cmd.current_dir(repo);
    })
}

fn link_force(home: &Path, repo: &Path, files: &[&str]) -> std::process::Output {
    run_wt(home, |cmd| {
        cmd.arg("link");
        cmd.arg("--force");
        cmd.args(files);
        cmd.current_dir(repo);
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
        err1.contains("linked .env"),
        "first run should create link, got: {err1}",
    );

    let out2 = wt_link(home.path(), &repo, &[".env"]);
    assert!(out2.status.success());
    let err2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        !err2.contains("linked .env"),
        "second run should not re-link, got: {err2}",
    );
    assert!(
        !err2.contains("skipped"),
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
        stderr.contains("cannot link .env: not found in ~/repo"),
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
fn links_with_a_single_worktree() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    assert!(repo.join(".env").is_symlink());
    assert_eq!(
        std::fs::read_to_string(repo.join(".env")).unwrap(),
        "SECRET=abc"
    );
}

#[test]
fn warns_when_regular_file_exists() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".git/info/exclude"), ".env\n").unwrap();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-conflict");

    std::fs::write(wt_path.join(".env"), "LOCAL=xyz").unwrap();

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert!(output.status.success(), "should still exit 0");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "moved .env into ~/repo/.git/wt/links/.env\n\
         skipped .env in 'feat-conflict': already exists, use --force to replace\n"
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
        stderr.contains("skipped .env"),
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
        stderr.contains("replaced .env in 'feat-force' with a link"),
        "should report replacing, got: {stderr}",
    );
    assert!(
        !stderr.contains("skipped"),
        "should not warn with --force, got: {stderr}",
    );

    let link = wt_path.join(".env");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "SECRET=abc");
}

#[test]
fn force_refuses_directory_conflict() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat-forcedir");

    let dest_dir = wt_path.join(".env");
    std::fs::create_dir(&dest_dir).unwrap();
    std::fs::write(dest_dir.join("old.txt"), "old").unwrap();

    let output = link_force(home.path(), &repo, &[".env"]);
    assert!(
        !output.status.success(),
        "wt link --force should fail on directory: {}",
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
    assert!(
        dest_dir.join("old.txt").exists(),
        "directory contents should be preserved"
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
        !stderr.contains("linked"),
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

#[test]
fn moves_file_into_git_dir_and_links_it_back() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();

    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());

    let stored = repo.join(".git/wt/links/.env");
    assert!(!stored.is_symlink());
    assert_eq!(std::fs::read_to_string(&stored).unwrap(), "SECRET=abc");
    assert_eq!(
        canonical(&std::fs::read_link(repo.join(".env")).unwrap()),
        canonical(&stored)
    );
}

#[test]
fn auto_links_on_new_worktree_creation() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let _wt1 = wt_new(home.path(), &repo, "feat-first");

    let link_out = wt_link(home.path(), &repo, &[".env"]);
    assert!(link_out.status.success());

    let wt2 = wt_new(home.path(), &repo, "feat-auto");

    let link = wt2.join(".env");
    assert!(
        link.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        ".env should be auto-linked in new worktree"
    );
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "SECRET=abc");
}

#[test]
fn auto_links_on_switch_creation() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let _wt1 = wt_new(home.path(), &repo, "feat-switch-first");

    let link_out = wt_link(home.path(), &repo, &[".env"]);
    assert!(link_out.status.success());

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["switch", "feat/switch-auto"]).current_dir(&repo);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let link = wt_path.join(".env");
    assert!(
        link.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        ".env should be auto-linked in switch-created worktree"
    );
}

#[test]
fn auto_links_from_primary_when_run_in_linked_worktree() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());
    let feat = wt_new(home.path(), &repo, "feat-a");

    let nested = wt_new(home.path(), &feat, "feat-b");

    let target = std::fs::read_link(nested.join(".env")).expect(".env should be auto-linked");
    assert_eq!(
        canonical(&target),
        canonical(&repo.join(".env")),
        "link should point at the primary worktree, not the worktree it was created from",
    );
}

#[test]
fn continues_linking_after_failure() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let blocked = wt_new(home.path(), &repo, "feat-blocked");
    let open = wt_new(home.path(), &repo, "feat-open");
    std::fs::create_dir(blocked.join(".env")).unwrap();

    let output = link_force(home.path(), &repo, &[".env"]);
    assert_exit_code(&output, 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot remove .env in") && stderr.ends_with("cannot link 1 file\n"),
        "got: {stderr}"
    );
    assert!(blocked.join(".env").is_dir());
    assert!(open.join(".env").is_symlink());
}

#[test]
fn skips_worktrees_whose_directory_is_missing() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let deleted = wt_new(home.path(), &repo, "feat");
    std::fs::remove_dir_all(&deleted).unwrap();

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    assert!(
        !deleted.exists(),
        "linking must not recreate a deleted worktree"
    );
}

#[test]
fn normalizes_paths_and_rejects_git_metadata() {
    let (home, repo) = setup();
    std::fs::create_dir(repo.join("cfg")).unwrap();
    std::fs::write(repo.join("cfg/local.env"), "LOCAL=1").unwrap();
    let wt = wt_new(home.path(), &repo, "feat");

    assert!(
        wt_link(home.path(), &repo, &["./cfg//local.env"])
            .status
            .success()
    );
    assert!(wt.join("cfg/local.env").is_symlink());
    assert!(repo.join(".git/wt/links/cfg/local.env").is_file());

    for bad in [".git", "", "."] {
        let output = link_force(home.path(), &repo, &[bad]);
        assert_exit_code(&output, 1);
    }
    assert!(wt.join(".git").is_file(), ".git must never be replaced");
}

#[test]
fn reports_worktrees_by_branch_and_detached_by_path() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    wt_new(home.path(), &repo, "feat-label");
    let detached = wt_new(home.path(), &repo, "to-detach");
    assert_git_success(&detached, &["checkout", "--detach"]);

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("linked .env in 'feat-label'\n"), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "linked .env in {}\n",
            tilde(home.path(), &detached)
        )),
        "{stderr}"
    );
}

#[test]
fn removing_the_worktree_linked_from_keeps_the_file() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".git/info/exclude"), ".env\n").unwrap();
    let source = wt_new(home.path(), &repo, "feat-a");
    let other = wt_new(home.path(), &repo, "feat-b");
    std::fs::write(source.join(".env"), "SECRET=abc").unwrap();
    assert!(wt_link(home.path(), &source, &[".env"]).status.success());

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["rm", "feat-a"]).current_dir(&repo);
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!source.exists());
    for wt in [&repo, &other] {
        assert_eq!(
            std::fs::read_to_string(wt.join(".env")).unwrap(),
            "SECRET=abc"
        );
    }
}

#[test]
fn refuses_tracked_files() {
    let (home, repo) = setup();
    std::fs::create_dir(repo.join("cfg")).unwrap();
    std::fs::write(repo.join("cfg/app.toml"), "k = 1").unwrap();
    assert_git_success(&repo, &["add", "cfg"]);

    let output = wt_link(home.path(), &repo, &["cfg/app.toml"]);
    assert_error(
        &output,
        1,
        "cannot link cfg/app.toml: tracked by git, only untracked files can be shared\n",
    );
    assert!(!repo.join("cfg/app.toml").is_symlink());
}

#[test]
fn links_a_path_given_twice_once() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".git/info/exclude"), ".env\n").unwrap();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();

    let output = wt_link(home.path(), &repo, &[".env", "./.env"]);
    assert!(output.status.success());
    assert_stderr_exact(&output, "moved .env into ~/repo/.git/wt/links/.env\n");
    assert_eq!(
        std::fs::read_to_string(repo.join(".env")).unwrap(),
        "SECRET=abc"
    );
}

#[test]
fn replaces_dangling_links_without_force() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat");
    create_symlink(&home.path().join("moved/.env"), &wt_path.join(".env"));

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env")).unwrap(),
        "SECRET=abc"
    );
}

#[test]
fn reports_when_git_cannot_tell_whether_a_file_is_tracked() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    std::fs::write(repo.join(".git/index"), "corrupt").unwrap();

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert_exit_code(&output, 1);
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("cannot check whether .env is tracked"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!repo.join(".env").is_symlink());
}

#[test]
fn warns_when_file_is_not_ignored() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert_exit_code(&output, 0);
    assert_stderr_exact(
        &output,
        ".env is not ignored by git, it will show as untracked in every worktree \
         and block `wt rm`, add it to .gitignore\n\
         moved .env into ~/repo/.git/wt/links/.env\n",
    );
}

#[test]
fn not_found_from_subdirectory_suggests_root_relative_path() {
    let (home, repo) = setup();
    std::fs::create_dir(repo.join("sub")).unwrap();
    std::fs::write(repo.join("sub/key.txt"), "k").unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.current_dir(repo.join("sub")).args(["link", "key.txt"]);
    });
    assert_error(
        &output,
        1,
        "cannot link key.txt: not found in ~/repo, paths are relative to the worktree root, \
         did you mean sub/key.txt?\n",
    );
}

#[test]
fn refuses_directories() {
    let (home, repo) = setup();
    std::fs::create_dir(repo.join("cfg")).unwrap();
    std::fs::write(repo.join("cfg/local.env"), "LOCAL=1").unwrap();

    let output = wt_link(home.path(), &repo, &["cfg"]);
    assert_error(
        &output,
        1,
        "cannot link cfg: is a directory, link the files inside it\n",
    );
    assert!(repo.join("cfg").is_dir());
    assert!(!repo.join(".git/wt/links/cfg").exists());
}

#[test]
fn auto_links_nested_files_on_new_worktree_creation() {
    let (home, repo) = setup();
    std::fs::create_dir(repo.join("cfg")).unwrap();
    std::fs::write(repo.join("cfg/local.env"), "LOCAL=1").unwrap();
    std::fs::write(repo.join(".env"), "SECRET=abc").unwrap();
    assert!(
        wt_link(home.path(), &repo, &["cfg/local.env", ".env"])
            .status
            .success()
    );

    let wt_path = wt_new(home.path(), &repo, "feat");

    assert_eq!(
        std::fs::read_to_string(wt_path.join("cfg/local.env")).unwrap(),
        "LOCAL=1"
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env")).unwrap(),
        "SECRET=abc"
    );
}

#[test]
fn rerunning_finishes_a_link_interrupted_after_the_move() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".git/info/exclude"), ".env\n").unwrap();
    let wt_path = wt_new(home.path(), &repo, "feat");
    let stored = repo.join(".git/wt/links/.env");
    std::fs::create_dir_all(stored.parent().unwrap()).unwrap();
    std::fs::write(&stored, "SECRET=abc").unwrap();

    let output = wt_link(home.path(), &repo, &[".env"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for wt in [&repo, &wt_path] {
        assert!(wt.join(".env").is_symlink());
        assert_eq!(
            std::fs::read_to_string(wt.join(".env")).unwrap(),
            "SECRET=abc"
        );
    }
}

#[test]
fn never_overwrites_the_shared_copy_with_a_differently_cased_name() {
    let (home, repo) = setup();
    std::fs::write(repo.join(".git/info/exclude"), ".env\n.Env\n").unwrap();
    std::fs::write(repo.join(".env"), "ORIGINAL").unwrap();
    if !repo.join(".Env").exists() {
        eprintln!("case-sensitive filesystem, skipping");
        return;
    }
    let other = wt_new(home.path(), &repo, "other");
    assert!(wt_link(home.path(), &repo, &[".env"]).status.success());
    std::fs::remove_file(other.join(".env")).unwrap();
    std::fs::write(other.join(".env"), "LOCAL").unwrap();

    let output = wt_link(home.path(), &other, &[".Env"]);
    assert_exit_code(&output, 1);
    assert_eq!(
        std::fs::read_to_string(repo.join(".git/wt/links/.env")).unwrap(),
        "ORIGINAL"
    );
    assert_eq!(
        std::fs::read_to_string(other.join(".env")).unwrap(),
        "LOCAL"
    );
}

#[test]
fn force_keeps_a_shared_file_reached_through_a_symlinked_parent() {
    let (home, repo) = setup();
    std::fs::create_dir(repo.join("config")).unwrap();
    std::fs::write(repo.join("config/a.env"), "PRECIOUS").unwrap();
    let other = wt_new(home.path(), &repo, "other");
    assert!(
        wt_link(home.path(), &repo, &["config/a.env"])
            .status
            .success()
    );
    std::fs::remove_dir_all(other.join("config")).unwrap();
    create_symlink(&repo.join(".git/wt/links/config"), &other.join("config"));

    let output = link_force(home.path(), &repo, &["config/a.env"]);
    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(repo.join(".git/wt/links/config/a.env")).unwrap(),
        "PRECIOUS"
    );
}
