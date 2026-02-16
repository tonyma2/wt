use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

pub fn wt_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wt"))
}

pub fn wt(home: &Path) -> Command {
    let mut cmd = wt_bin();
    cmd.env("HOME", home);
    cmd
}

pub fn run_wt(home: &Path, configure: impl FnOnce(&mut Command)) -> Output {
    let mut cmd = wt(home);
    configure(&mut cmd);
    cmd.output().unwrap()
}

pub fn git(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    cmd
}

pub fn assert_git_success(dir: &Path, args: &[&str]) {
    let status = git(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git failed to start");
    assert!(status.success(), "git {:?} failed", args);
}

pub fn assert_git_success_with(dir: &Path, configure: impl FnOnce(&mut Command)) {
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

pub fn assert_git_stdout_success(dir: &Path, args: &[&str]) -> String {
    let output = git(dir).args(args).output().expect("git failed to start");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn assert_branch_absent(repo: &Path, branch: &str) {
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

pub fn assert_branch_present(repo: &Path, branch: &str) {
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

pub fn setup() -> (TempDir, PathBuf) {
    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);
    (home, repo)
}

pub fn init_repo(dir: &Path) {
    assert_git_success(dir, &["init", "-b", "main"]);
    assert_git_success(dir, &["config", "user.name", "Test"]);
    assert_git_success(dir, &["config", "user.email", "t@t"]);
    assert_git_success(dir, &["commit", "--allow-empty", "-m", "init"]);
}

pub fn setup_with_origin() -> (TempDir, PathBuf, PathBuf) {
    let (home, repo) = setup();
    let origin = home.path().join("origin.git");
    init_bare_repo(&origin);
    assert_git_success_with(&repo, |cmd| {
        cmd.args(["remote", "add", "origin"]).arg(&origin);
    });
    assert_git_success(&repo, &["push", "-u", "origin", "main"]);
    assert_git_success(&repo, &["fetch", "--prune", "origin"]);
    (home, repo, origin)
}

pub fn init_bare_repo(path: &Path) {
    let status = Command::new("git")
        .args(["init", "--bare"])
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git failed to start");
    assert!(status.success(), "git init --bare failed");
}

pub fn wt_new(home: &Path, repo: &Path, branch: &str) -> PathBuf {
    let output = run_wt(home, |cmd| {
        cmd.args(["new", "-c", branch, "--repo"]).arg(repo);
    });
    assert!(
        output.status.success(),
        "wt new {branch} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    parse_wt_new_path(&output)
}

pub fn wt_checkout(home: &Path, repo: &Path, name: &str) -> PathBuf {
    let output = run_wt(home, |cmd| {
        cmd.args(["new", name, "--repo"]).arg(repo);
    });
    assert!(
        output.status.success(),
        "wt new {name} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    parse_wt_new_path(&output)
}

pub fn parse_wt_new_path(output: &Output) -> PathBuf {
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

pub fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn normalize_home_paths(output: &str, home: &Path) -> String {
    let mut normalized = output.to_string();
    if let Ok(canon_home) = home.canonicalize() {
        normalized = normalized.replace(canon_home.to_string_lossy().as_ref(), "$HOME");
    }
    normalized.replace(home.to_string_lossy().as_ref(), "$HOME")
}

pub fn assert_exit_code(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "expected exit code {code}, got {:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

pub fn assert_stdout_empty(output: &Output) {
    assert!(
        output.stdout.is_empty(),
        "stdout should be empty, got: {}",
        String::from_utf8_lossy(&output.stdout),
    );
}

pub fn assert_stderr_empty(output: &Output) {
    assert!(
        output.stderr.is_empty(),
        "stderr should be empty, got: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

pub fn assert_stderr_exact(output: &Output, expected: &str) {
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        expected,
        "unexpected stderr",
    );
}

pub fn assert_error(output: &Output, code: i32, expected_stderr: &str) {
    assert_exit_code(output, code);
    assert_stdout_empty(output);
    assert_stderr_exact(output, expected_stderr);
}
