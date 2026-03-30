pub mod common;

use common::*;

#[test]
fn zsh_init_includes_dynamic_worktree_helpers() {
    let output = wt_bin().args(["init", "zsh"]).output().unwrap();

    assert!(
        output.status.success(),
        "wt init zsh failed: {}",
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
    assert!(stdout.contains("cmd+=(worktree list --porcelain)"));
    assert!(stdout.contains("for-each-ref"));
}

#[test]
fn zsh_init_has_link_unlink_file_completions() {
    let output = wt_bin().args(["init", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_wt_link_files()"));
    assert!(stdout.contains("_wt_unlink_files()"));
    assert!(!stdout.contains("Files or directories to link:_default"));
    assert!(!stdout.contains("Files or directories to unlink:_default"));
}

#[test]
fn zsh_init_strips_compdef_header() {
    let output = wt_bin().args(["init", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.starts_with("#compdef"));
    assert!(stdout.contains("compdef _wt wt"));
}

#[test]
fn zsh_init_includes_wrapper() {
    let output = wt_bin().args(["init", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wt() {"));
    assert!(stdout.contains("mktemp"));
    assert!(stdout.contains("__WT_CD="));
    assert!(stdout.contains("rm -f"));
    assert!(stdout.contains("new|n|switch|s|clone|cl)"));
    assert!(stdout.contains(r#"cd -- "$out""#));
}

#[test]
fn bash_init_excludes_zsh_specific_helpers() {
    let output = wt_bin().args(["init", "bash"]).output().unwrap();

    assert!(
        output.status.success(),
        "wt init bash failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("_wt_path_branches()"));
    assert!(!stdout.contains("_wt_collect_worktree_rows()"));
    assert!(!stdout.contains("_wt_switch_targets()"));
    assert!(!stdout.contains("_wt_new_name()"));
    assert!(!stdout.contains("_wt_new_base()"));
}

#[test]
fn bash_init_includes_wrapper() {
    let output = wt_bin().args(["init", "bash"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("wt() {"));
    assert!(stdout.contains("mktemp"));
    assert!(stdout.contains("__WT_CD="));
    assert!(stdout.contains("rm -f"));
    assert!(stdout.contains("new|n|switch|s|clone|cl)"));
    assert!(stdout.contains(r#"cd -- "$out""#));
}

#[test]
fn fish_init_includes_wrapper() {
    let output = wt_bin().args(["init", "fish"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("function wt --wraps=wt"));
    assert!(stdout.contains("mktemp"));
    assert!(stdout.contains("__WT_CD="));
    assert!(stdout.contains("rm -f"));
    assert!(stdout.contains("command wt $argv"));
    assert!(stdout.contains("case new n switch s"));
    assert!(stdout.contains("and cd -- $out"));
}

#[test]
fn completions_subcommand_is_removed() {
    let output = wt_bin().args(["completions", "zsh"]).output().unwrap();
    assert_exit_code(&output, 2);
}
