use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn git_err(context: impl AsRef<str>, output: &Output) -> String {
    let context = context.as_ref();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let msg = line
        .strip_prefix("fatal: ")
        .or_else(|| line.strip_prefix("error: "))
        .unwrap_or(line);
    if msg.is_empty() {
        context.into()
    } else {
        format!("{context}: {msg}")
    }
}

pub struct Git {
    repo: PathBuf,
}

impl Git {
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        Self { repo: repo.into() }
    }

    fn cmd_in(path: &Path) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(path);
        cmd
    }

    fn cmd(&self) -> Command {
        Self::cmd_in(&self.repo)
    }

    pub fn find_repo(path: Option<&Path>) -> Result<PathBuf, String> {
        let mut cmd = Command::new("git");
        if let Some(p) = path {
            cmd.arg("-C").arg(p);
        }
        cmd.args(["rev-parse", "--show-toplevel"]);
        let output = cmd.output().map_err(|e| format!("cannot run git: {e}"))?;
        if !output.status.success() {
            return Err("not a git repository, use --repo or run inside one".into());
        }
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(s))
    }

    pub fn remote_url(&self, remote: &str) -> Option<String> {
        let output = self
            .cmd()
            .args(["remote", "get-url", remote])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    pub fn has_remote(&self, remote: &str) -> bool {
        self.remote_url(remote).is_some()
    }

    pub fn fetch_remote(&self, remote: &str) -> Result<(), String> {
        let status = self
            .cmd()
            .args(["fetch", "--prune", remote])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| format!("cannot run git fetch: {e}"))?;
        if !status.success() {
            // detail already visible on inherited stderr
            return Err(format!("cannot fetch from '{remote}'"));
        }
        Ok(())
    }

    pub fn base_ref(&self) -> Result<String, String> {
        let output = self
            .cmd()
            .args(["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"])
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;

        if output.status.success() {
            let head_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(branch) = head_ref.strip_prefix("refs/remotes/origin/")
                && self.ref_exists(&format!("refs/remotes/origin/{branch}"))
            {
                return Ok(format!("origin/{branch}"));
            }
        }

        for name in ["main", "master"] {
            if self.ref_exists(&format!("refs/remotes/origin/{name}")) {
                return Ok(format!("origin/{name}"));
            }
        }

        Err(
            "cannot determine default branch (tried origin/HEAD, origin/main, origin/master)"
                .into(),
        )
    }

    pub fn ref_exists(&self, refname: &str) -> bool {
        self.cmd()
            .args(["show-ref", "--verify", "--quiet", refname])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    pub fn has_local_branch(&self, name: &str) -> bool {
        self.ref_exists(&format!("refs/heads/{name}"))
    }

    pub fn local_branches(&self) -> Vec<String> {
        let output = self
            .cmd()
            .args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"])
            .output()
            .ok();
        match output {
            Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_string)
                .collect(),
            // Best-effort: fuzzy matching is advisory, so silently degrade on failure
            _ => vec![],
        }
    }

    pub fn remotes_with_branch(&self, name: &str) -> Result<Vec<String>, String> {
        if name == "HEAD" {
            return Ok(vec![]);
        }
        let output = self
            .cmd()
            .args(["remote"])
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git remote: {e}"))?;
        if !output.status.success() {
            return Err(git_err("cannot list remotes", &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|remote| self.ref_exists(&format!("refs/remotes/{remote}/{name}")))
            .map(str::to_string)
            .collect())
    }

    pub fn add_worktree(
        &self,
        branch: &str,
        dest: &Path,
        base_ref: Option<&str>,
    ) -> Result<(), String> {
        let mut cmd = self.cmd();
        cmd.args(["worktree", "add", "--quiet", "-b", branch])
            .arg(dest);
        if let Some(base) = base_ref {
            cmd.arg(base);
        }
        let output = cmd
            .stdout(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git worktree add: {e}"))?;
        if !output.status.success() {
            return Err(git_err("cannot create worktree", &output));
        }
        Ok(())
    }

    pub fn checkout_worktree(&self, branch: &str, dest: &Path) -> Result<(), String> {
        let output = self
            .cmd()
            .args(["worktree", "add", "--quiet"])
            .arg(dest)
            .arg(branch)
            .stdout(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git worktree add: {e}"))?;
        if !output.status.success() {
            return Err(git_err("cannot create worktree", &output));
        }
        Ok(())
    }

    pub fn list_worktrees(&self) -> Result<String, String> {
        let output = self
            .cmd()
            .args(["worktree", "list", "--porcelain"])
            .output()
            .map_err(|e| format!("cannot run git worktree list: {e}"))?;
        if !output.status.success() {
            return Err(git_err("cannot list worktrees", &output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<(), String> {
        let mut cmd = self.cmd();
        cmd.args(["worktree", "remove"]);
        if force {
            cmd.arg("--force");
        }
        cmd.arg(path);
        cmd.stdout(Stdio::null());
        let output = cmd
            .output()
            .map_err(|e| format!("cannot run git worktree remove: {e}"))?;
        if !output.status.success() {
            return Err(git_err(
                format!("cannot remove worktree: {}", path.display()),
                &output,
            ));
        }
        Ok(())
    }

    pub fn delete_branch(&self, branch: &str, force: bool) -> Result<(), String> {
        let flag = if force { "-D" } else { "-d" };
        let output = self
            .cmd()
            .args(["branch", flag, "--quiet", branch])
            .stdout(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git branch: {e}"))?;
        if !output.status.success() {
            let action = if force { "force-delete" } else { "delete" };
            return Err(git_err(
                format!("worktree removed but cannot {action} branch '{branch}'"),
                &output,
            ));
        }
        Ok(())
    }

    pub fn prune_worktrees(&self, dry_run: bool) -> Result<String, String> {
        let mut cmd = self.cmd();
        cmd.args(["worktree", "prune", "--verbose"]);
        if dry_run {
            cmd.arg("--dry-run");
        }
        let output = cmd
            .output()
            .map_err(|e| format!("cannot run git worktree prune: {e}"))?;
        if !output.status.success() {
            return Err(git_err("cannot prune worktree metadata", &output));
        }
        Ok(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }

    pub fn is_dirty(&self, worktree_path: &Path) -> bool {
        Self::cmd_in(worktree_path)
            .args(["status", "--porcelain", "--untracked-files=normal"])
            .stderr(Stdio::null())
            .output()
            .map_or(true, |o| !o.stdout.is_empty())
    }

    pub fn is_branch_merged(&self, branch: &str) -> bool {
        let branch_ref = format!("refs/heads/{branch}");

        if let Some(upstream) = self.upstream_for(&branch_ref)
            && self.rev_resolves(&upstream)
        {
            return self.is_ancestor(&branch_ref, &upstream);
        }

        self.is_ancestor(&branch_ref, "HEAD")
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        self.cmd()
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    pub fn rev_parse(&self, refname: &str) -> Option<String> {
        let output = self
            .cmd()
            .args(["rev-parse", "--verify", "--quiet", refname])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!sha.is_empty()).then_some(sha)
    }

    pub fn rev_resolves(&self, refname: &str) -> bool {
        self.cmd()
            .args(["rev-parse", "--verify", "--quiet", refname])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    pub fn worktree_status(worktree_path: &Path) -> (bool, Option<u64>, Option<u64>) {
        let output = Self::cmd_in(worktree_path)
            .args([
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=normal",
            ])
            .stderr(Stdio::null())
            .output();
        let Ok(output) = output else {
            return (true, None, None);
        };
        if !output.status.success() {
            return (true, None, None);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        parse_porcelain_status(&text)
    }

    pub fn is_upstream_gone(&self, branch: &str) -> bool {
        let branch_ref = format!("refs/heads/{branch}");
        self.upstream_for(&branch_ref)
            .is_some_and(|upstream| !self.rev_resolves(&upstream))
    }

    pub fn upstream_remote(&self, branch: &str) -> Option<String> {
        let output = self
            .cmd()
            .args(["config", "--get", &format!("branch.{branch}.remote")])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!remote.is_empty()).then_some(remote)
    }

    pub fn bare_clone(url: &str, dest: &Path) -> Result<(), String> {
        let status = Command::new("git")
            .args(["clone", "--bare", url])
            .arg(dest)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| format!("cannot run git clone: {e}"))?;
        if !status.success() {
            // detail already visible on inherited stderr
            return Err("cannot clone repository".to_string());
        }
        Ok(())
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<(), String> {
        let output = self
            .cmd()
            .args(["config", key, value])
            .stdout(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git config: {e}"))?;
        if !output.status.success() {
            return Err(git_err(format!("cannot set config '{key}'"), &output));
        }
        Ok(())
    }

    pub fn set_remote_head(&self, remote: &str) -> Result<(), String> {
        let output = self
            .cmd()
            .args(["remote", "set-head", remote, "--auto"])
            .stdout(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git remote: {e}"))?;
        if !output.status.success() {
            return Err(git_err(
                format!("cannot detect default branch from '{remote}'"),
                &output,
            ));
        }
        Ok(())
    }

    fn upstream_for(&self, refspec: &str) -> Option<String> {
        let output = self
            .cmd()
            .args(["for-each-ref", "--format=%(upstream:short)", refspec])
            .output()
            .ok()?;
        let upstream = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!upstream.is_empty()).then_some(upstream)
    }
}

fn parse_porcelain_status(text: &str) -> (bool, Option<u64>, Option<u64>) {
    let mut dirty = false;
    let mut ahead = None;
    let mut behind = None;
    for line in text.lines() {
        if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut parts = ab.split_whitespace();
            ahead = parts
                .next()
                .and_then(|s| s.strip_prefix('+'))
                .and_then(|s| s.parse().ok());
            behind = parts
                .next()
                .and_then(|s| s.strip_prefix('-'))
                .and_then(|s| s.parse().ok());
        } else if !line.starts_with('#') {
            dirty = true;
        }
    }
    (dirty, ahead, behind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Output;

    fn fake_output(stderr: &str) -> Output {
        Output {
            status: std::process::ExitStatus::default(),
            stdout: vec![],
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn git_err_strips_fatal_prefix() {
        let out = fake_output("fatal: invalid reference: aaaa\n");
        assert_eq!(
            git_err("cannot create worktree", &out),
            "cannot create worktree: invalid reference: aaaa"
        );
    }

    #[test]
    fn git_err_strips_error_prefix() {
        let out = fake_output("error: branch 'x' not found\n");
        assert_eq!(
            git_err("cannot delete branch", &out),
            "cannot delete branch: branch 'x' not found"
        );
    }

    #[test]
    fn git_err_preserves_warning_prefix() {
        let out = fake_output("warning: something unexpected\n");
        assert_eq!(
            git_err("cannot remove worktree", &out),
            "cannot remove worktree: warning: something unexpected"
        );
    }

    #[test]
    fn git_err_empty_stderr_returns_context_only() {
        let out = fake_output("");
        assert_eq!(
            git_err("cannot list worktrees", &out),
            "cannot list worktrees"
        );
    }

    #[test]
    fn git_err_multiline_uses_first_nonempty_line() {
        let out = fake_output("\nfatal: bad object: abc\nhint: use --force\n");
        assert_eq!(
            git_err("cannot create worktree", &out),
            "cannot create worktree: bad object: abc"
        );
    }

    #[test]
    fn parse_status_clean_with_upstream() {
        let text = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        assert_eq!(parse_porcelain_status(text), (false, Some(0), Some(0)));
    }

    #[test]
    fn parse_status_ahead_behind() {
        let text = "# branch.oid abc123\n# branch.head feat\n# branch.ab +3 -1\n";
        assert_eq!(parse_porcelain_status(text), (false, Some(3), Some(1)));
    }

    #[test]
    fn parse_status_dirty_with_changes() {
        let text = "# branch.oid abc123\n# branch.head main\n# branch.ab +0 -0\n1 .M N... 100644 100644 100644 abc def src/main.rs\n";
        assert_eq!(parse_porcelain_status(text), (true, Some(0), Some(0)));
    }

    #[test]
    fn parse_status_dirty_untracked() {
        let text = "# branch.oid abc123\n# branch.head main\n? newfile.txt\n";
        assert_eq!(parse_porcelain_status(text), (true, None, None));
    }

    #[test]
    fn parse_status_no_upstream() {
        let text = "# branch.oid abc123\n# branch.head main\n";
        assert_eq!(parse_porcelain_status(text), (false, None, None));
    }

    #[test]
    fn parse_status_detached_head() {
        let text = "# branch.oid abc123\n# branch.head (detached)\n";
        assert_eq!(parse_porcelain_status(text), (false, None, None));
    }

    #[test]
    fn parse_status_empty_output() {
        assert_eq!(parse_porcelain_status(""), (false, None, None));
    }
}
