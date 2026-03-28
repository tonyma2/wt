mod common;

use std::path::Path;
use std::process::Stdio;

use common::*;

fn setup_origin() -> (tempfile::TempDir, std::path::PathBuf) {
    let home = tempfile::TempDir::new().unwrap();
    let origin = home.path().join("origin");
    std::fs::create_dir(&origin).unwrap();
    init_repo(&origin);
    (home, origin)
}

fn repos_dir(home: &Path) -> std::path::PathBuf {
    home.join(".wt").join("repos")
}

fn worktrees_dir(home: &Path) -> std::path::PathBuf {
    home.join(".wt").join("worktrees")
}

fn find_subdirs(base: &Path) -> Vec<std::path::PathBuf> {
    if !base.exists() {
        return vec![];
    }
    std::fs::read_dir(base)
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .map(|e| e.path())
        .collect()
}

fn find_repo_under(base: &Path) -> std::path::PathBuf {
    let id_dirs = find_subdirs(base);
    assert_eq!(
        id_dirs.len(),
        1,
        "expected 1 id dir under {}",
        base.display()
    );
    let children = find_subdirs(&id_dirs[0]);
    assert_eq!(
        children.len(),
        1,
        "expected 1 repo dir under {}",
        id_dirs[0].display()
    );
    children[0].clone()
}

#[test]
fn clone_creates_worktree_and_bare_repo() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(
        output.status.success(),
        "wt clone failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let wt_path = parse_wt_new_path(&output);
    assert!(wt_path.exists());

    let bare_repo = find_repo_under(&repos_dir(home.path()));
    assert!(
        bare_repo.join("HEAD").exists(),
        "bare repo should have HEAD"
    );
    assert!(
        !bare_repo.join(".git").exists(),
        "bare repo should not have .git subdir"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cloning"), "stderr should mention cloning");
    assert!(
        stderr.contains("checked out 'main'"),
        "stderr should mention checked out branch"
    );
}

#[test]
fn clone_worktree_is_on_default_branch() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let branch = assert_git_stdout_success(&wt_path, &["branch", "--show-current"]);
    assert_eq!(branch.trim(), "main");
}

#[test]
fn clone_fixes_fetch_refspec() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());

    let bare_repo = find_repo_under(&repos_dir(home.path()));
    let refspec =
        assert_git_stdout_success(&bare_repo, &["config", "--get", "remote.origin.fetch"]);
    assert_eq!(refspec.trim(), "+refs/heads/*:refs/remotes/origin/*");
}

#[test]
fn clone_has_remote_tracking_branches() {
    let (home, origin) = setup_origin();
    assert_git_success(&origin, &["checkout", "-b", "feature"]);
    assert_git_success(
        &origin,
        &["commit", "--allow-empty", "-m", "feature commit"],
    );
    assert_git_success(&origin, &["checkout", "main"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let refs = assert_git_stdout_success(&wt_path, &["branch", "-r"]);
    assert!(refs.contains("origin/main"), "should have origin/main");
    assert!(
        refs.contains("origin/feature"),
        "should have origin/feature"
    );
}

#[test]
fn clone_invalid_url_fails_cleanly() {
    let home = tempfile::TempDir::new().unwrap();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone", "/nonexistent/path/to/repo.git"]);
    });
    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot clone"), "stderr: {stderr}");

    assert!(find_subdirs(&repos_dir(home.path())).is_empty() || !repos_dir(home.path()).exists());
    assert!(
        find_subdirs(&worktrees_dir(home.path())).is_empty()
            || !worktrees_dir(home.path()).exists()
    );
}

#[test]
fn clone_empty_url_fails() {
    let home = tempfile::TempDir::new().unwrap();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone", ""]);
    });
    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot determine repo name"),
        "stderr: {stderr}"
    );
}

#[test]
fn wt_new_works_from_cloned_worktree() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["new", "-c", "feature", "--repo"]).arg(&wt_path);
    });
    assert!(
        output.status.success(),
        "wt new from cloned worktree failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let feature_path = parse_wt_new_path(&output);
    let branch = assert_git_stdout_success(&feature_path, &["branch", "--show-current"]);
    assert_eq!(branch.trim(), "feature");
}

#[test]
fn list_from_cloned_worktree() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--repo"]).arg(&wt_path);
    });
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("main"), "list should show main branch");
}

#[test]
fn list_all_discovers_cloned_repo() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["list", "--all"]);
    });
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("main"),
        "list --all should show main branch"
    );
}

#[test]
fn prune_discovers_bare_repo_worktrees() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["new", "-c", "stale-branch", "--repo"])
            .arg(&wt_path);
    });
    assert!(output.status.success());
    let stale_path = parse_wt_new_path(&output);
    std::fs::remove_dir_all(&stale_path).unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["prune"]);
    });
    assert!(
        output.status.success(),
        "prune failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn rm_works_from_cloned_worktree() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["new", "-c", "to-remove", "--repo"]).arg(&wt_path);
    });
    assert!(output.status.success());

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["rm", "to-remove", "--repo"]).arg(&wt_path);
    });
    assert!(
        output.status.success(),
        "rm failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn link_skips_bare_entry_as_primary() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let primary_wt = parse_wt_new_path(&output);

    std::fs::write(primary_wt.join(".env"), "SECRET=1").unwrap();

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["new", "-c", "feature", "--repo"])
            .arg(&primary_wt);
    });
    assert!(output.status.success());
    let feature_wt = parse_wt_new_path(&output);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["link", ".env", "--repo"]).arg(&primary_wt);
    });
    assert!(
        output.status.success(),
        "link failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let link_dest = feature_wt.join(".env");
    assert!(link_dest.exists(), ".env should exist in feature worktree");
    let target = std::fs::read_link(&link_dest).unwrap();
    assert_eq!(canonical(&target), canonical(&primary_wt.join(".env")),);
}

#[test]
fn no_cd_hint_when_stdout_not_tty() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin).stdout(Stdio::piped());
    });
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("cd \"$(wt path"),
        "should not print cd hint when stdout piped"
    );
}

#[test]
fn clone_uses_repo_name_for_directory() {
    let (home, origin) = setup_origin();
    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(output.status.success());
    let wt_path = parse_wt_new_path(&output);
    let dir_name = wt_path.file_name().unwrap().to_string_lossy();
    assert_eq!(dir_name, "origin", "worktree dir should match repo name");
}

#[test]
fn clone_master_fallback() {
    let home = tempfile::TempDir::new().unwrap();
    let origin = home.path().join("origin");
    std::fs::create_dir(&origin).unwrap();
    assert_git_success(&origin, &["init", "-b", "master"]);
    assert_git_success(&origin, &["config", "user.name", "Test"]);
    assert_git_success(&origin, &["config", "user.email", "t@t"]);
    assert_git_success(&origin, &["commit", "--allow-empty", "-m", "init"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert!(
        output.status.success(),
        "wt clone failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let wt_path = parse_wt_new_path(&output);

    let branch = assert_git_stdout_success(&wt_path, &["branch", "--show-current"]);
    assert_eq!(branch.trim(), "master");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("checked out 'master'"));
}

#[test]
fn clone_empty_repo_fails_and_cleans_up() {
    let home = tempfile::TempDir::new().unwrap();
    let origin = home.path().join("empty-origin");
    std::fs::create_dir(&origin).unwrap();
    // Create a repo with no commits — bare clone will succeed but
    // base_ref() will fail because there are no branches
    assert_git_success(&origin, &["init", "-b", "main"]);

    let output = run_wt(home.path(), |cmd| {
        cmd.args(["clone"]).arg(&origin);
    });
    assert_exit_code(&output, 1);
    assert_stdout_empty(&output);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot determine default branch"),
        "stderr: {stderr}"
    );

    // bare repo should be cleaned up after failure
    assert!(
        find_subdirs(&repos_dir(home.path())).is_empty() || !repos_dir(home.path()).exists(),
        "bare repo should be cleaned up"
    );
    assert!(
        find_subdirs(&worktrees_dir(home.path())).is_empty()
            || !worktrees_dir(home.path()).exists(),
        "no worktree dirs should exist"
    );
}
