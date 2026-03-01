pub mod common;

use common::*;

#[test]
fn zsh_completion_includes_dynamic_worktree_helpers() {
    let output = wt_bin().args(["completions", "zsh"]).output().unwrap();

    assert!(
        output.status.success(),
        "wt completions zsh failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(output.stderr.is_empty(), "stderr should be empty");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_wt_path_branches()"));
    assert!(stdout.contains("_wt_remove_targets()"));
    assert!(stdout.contains("_wt_collect_worktree_rows()"));
    assert!(stdout.contains("_wt_extract_repo_args()"));
    assert!(stdout.contains("_wt_collect_local_branches()"));
    assert!(stdout.contains("_wt_switch_targets()"));
    assert!(stdout.contains("_wt_new_name()"));
    assert!(stdout.contains("_wt_new_base()"));
    assert!(stdout.contains("_wt_link_files()"));
    assert!(stdout.contains("_wt_unlink_files()"));
    assert!(stdout.contains("command wt list --porcelain"));
    assert!(stdout.contains("for-each-ref"));
}

#[test]
fn zsh_completion_has_link_unlink_file_completions() {
    let output = wt_bin().args(["completions", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_wt_link_files()"));
    assert!(stdout.contains("_wt_unlink_files()"));
    assert!(!stdout.contains("Files or directories to link:_default"));
    assert!(!stdout.contains("Files or directories to unlink:_default"));
}

#[test]
fn bash_completion_excludes_zsh_specific_helpers() {
    let output = wt_bin().args(["completions", "bash"]).output().unwrap();

    assert!(
        output.status.success(),
        "wt completions bash failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("_wt_path_branches()"));
    assert!(!stdout.contains("_wt_collect_worktree_rows()"));
    assert!(!stdout.contains("_wt_switch_targets()"));
    assert!(!stdout.contains("_wt_new_name()"));
    assert!(!stdout.contains("_wt_new_base()"));
}
