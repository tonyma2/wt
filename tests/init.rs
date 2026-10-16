pub mod common;

use common::*;

#[test]
fn init_prints_script_for_each_shell() {
    for shell in ["zsh", "bash", "fish"] {
        let output = wt_bin().args(["init", shell]).output().unwrap();
        assert!(output.status.success(), "wt init {shell} failed");
        assert!(output.stderr.is_empty(), "wt init {shell} wrote to stderr");
        assert!(!output.stdout.is_empty(), "wt init {shell} printed nothing");
    }
}

fn run_wrapper(
    home: &std::path::Path,
    dir: &std::path::Path,
    script: &str,
) -> std::process::Output {
    let bin_dir = std::path::Path::new(env!("CARGO_BIN_EXE_wt"))
        .parent()
        .unwrap();
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    std::process::Command::new("bash")
        .args(["--norc", "--noprofile", "-c"])
        .arg(format!("eval \"$(wt init bash)\"\n{script}"))
        .current_dir(dir)
        .env("HOME", home)
        .env("PATH", path)
        .output()
        .unwrap()
}

#[test]
fn bash_wrapper_passes_help_through_and_cds_on_create() {
    let (home, repo) = setup();

    let output = run_wrapper(home.path(), &repo, "wt switch --help");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage"));

    let output = run_wrapper(home.path(), &repo, "wt s feat >/dev/null 2>&1 && pwd");
    assert!(output.status.success());
    let cwd = std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    assert!(
        cwd.starts_with(canonical(&home.path().join(".wt/worktrees"))),
        "{cwd:?}"
    );

    assert_git_success(&repo, &["tag", "v1.0"]);
    let output = run_wrapper(home.path(), &repo, "wt switch v1.0");
    assert_exit_code(&output, 1);
}

// Runs a zsh completer with `compadd` stubbed to print the offered values. Returns `None` when
// zsh is not installed.
fn complete_zsh(
    home: &std::path::Path,
    dir: &std::path::Path,
    completer: &str,
    words: &[&str],
) -> Option<std::process::Output> {
    let bin_dir = std::path::Path::new(env!("CARGO_BIN_EXE_wt"))
        .parent()
        .unwrap();
    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let words: Vec<String> = words.iter().map(|w| format!("'{w}'")).collect();
    let script = format!(
        "autoload -U compinit && compinit -u -D\n\
         eval \"$(wt init zsh)\"\n\
         compadd() {{ local i=${{@[(i)--]}}; print -rl -- \"${{@[i+1,-1]}}\"; }}\n\
         words=({}); CURRENT=$#words\n\
         {completer}",
        words.join(" ")
    );
    std::process::Command::new("zsh")
        .args(["-f", "-c", &script])
        .current_dir(dir)
        .env("HOME", home)
        .env("PATH", path)
        .output()
        .ok()
}

#[test]
fn zsh_completion_expands_tilde_in_repo_flag() {
    let (home, repo) = setup();
    wt_new(home.path(), &repo, "feat");

    for words in [
        ["wt", "path", "--repo", "~/repo", ""].as_slice(),
        ["wt", "path", "--repo=~/repo", ""].as_slice(),
    ] {
        let Some(output) = complete_zsh(home.path(), home.path(), "_wt_path_branches", words)
        else {
            eprintln!("zsh not installed, skipping");
            return;
        };
        assert!(output.status.success(), "{words:?}");
        let offered = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            offered.lines().collect::<Vec<_>>(),
            ["main", "feat"],
            "{words:?}"
        );
    }
}

fn offered(output: &std::process::Output) -> Vec<String> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn zsh_remove_completion_skips_main_and_typed_worktrees() {
    let (home, repo) = setup();
    let feat = wt_new(home.path(), &repo, "feat");
    wt_new(home.path(), &repo, "fix");
    let feat = feat.to_str().unwrap();

    for (words, expected) in [
        (["wt", "rm", ""].as_slice(), ["feat", "fix"].as_slice()),
        (["wt", "rm", "fix", ""].as_slice(), ["feat"].as_slice()),
        (["wt", "rm", feat, ""].as_slice(), ["fix"].as_slice()),
    ] {
        let Some(output) = complete_zsh(home.path(), &repo, "_wt_remove_targets", words) else {
            eprintln!("zsh not installed, skipping");
            return;
        };
        let mut offered = offered(&output);
        offered.sort();
        assert_eq!(offered, expected, "{words:?}");
    }
}

#[test]
fn zsh_switch_offers_worktrees_then_other_branches_once() {
    let (home, repo, _origin) = setup_with_origin();
    wt_new(home.path(), &repo, "feat");
    assert_git_success(&repo, &["branch", "local-only"]);
    assert_git_success(&repo, &["branch", "remote-only"]);
    assert_git_success(&repo, &["push", "origin", "remote-only", "feat"]);
    assert_git_success(&repo, &["branch", "-D", "remote-only"]);
    assert_git_success(&repo, &["tag", "v1.0"]);

    let Some(output) = complete_zsh(home.path(), &repo, "_wt_switch_targets", &["wt", "s", ""])
    else {
        eprintln!("zsh not installed, skipping");
        return;
    };
    assert_eq!(
        offered(&output),
        ["main", "feat", "local-only", "remote-only"]
    );
}

#[test]
fn bash_completes_branches_and_worktrees() {
    let (home, repo) = setup();
    assert_git_success(&repo, &["branch", "feat/a"]);
    assert_git_success(&repo, &["tag", "v1.0"]);
    wt_new(home.path(), &repo, "feat/b");

    let complete = |words: &str| {
        let script = format!(
            "COMP_WORDS=({words}); COMP_CWORD=$(( ${{#COMP_WORDS[@]}} - 1 ))\n\
             _wt_complete wt \"${{COMP_WORDS[COMP_CWORD]}}\" \"${{COMP_WORDS[COMP_CWORD-1]}}\"\n\
             printf '%s\\n' \"${{COMPREPLY[@]}}\""
        );
        let output = run_wrapper(home.path(), &repo, &script);
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    assert_eq!(complete("wt s ''"), "feat/a\nfeat/b\nmain\n");
    assert_eq!(complete("wt s -c f"), "feat/a\nfeat/b\n");
    assert_eq!(complete("wt s x ''"), "feat/a\nfeat/b\nmain\nv1.0\n");
    assert_eq!(complete("wt rm ''"), "main\nfeat/b\n");
    assert!(complete("wt s --").contains("--create"));
}
