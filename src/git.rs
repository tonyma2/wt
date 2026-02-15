use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn stderr_msg(output: &Output) -> String {
    let s = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if s.is_empty() {
        "unknown error".into()
    } else {
        s
    }
}

pub struct Git {
    repo: PathBuf,
}

impl Git {
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        Self { repo: repo.into() }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&self.repo);
        cmd
    }

    pub fn find_repo(path: Option<&Path>) -> Result<PathBuf, String> {
        let mut cmd = Command::new("git");
        if let Some(p) = path {
            cmd.arg("-C").arg(p);
        }
        cmd.args(["rev-parse", "--show-toplevel"]);
        let output = cmd.output().map_err(|e| format!("cannot run git: {e}"))?;
        if !output.status.success() {
            return Err("not a git repository; use --repo or run inside one".into());
        }
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(s))
    }

    pub fn has_origin(&self) -> bool {
        self.cmd()
            .args(["remote", "get-url", "origin"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    pub fn fetch_origin(&self) -> Result<(), String> {
        let output = self
            .cmd()
            .args(["fetch", "--prune", "--quiet", "origin"])
            .stdout(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run git fetch: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "cannot fetch from 'origin': {}",
                stderr_msg(&output)
            ));
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
            return Err(format!("cannot create worktree: {}", stderr_msg(&output)));
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
            return Err(format!("cannot create worktree: {}", stderr_msg(&output)));
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
            return Err(format!("cannot list worktrees: {}", stderr_msg(&output)));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
            return Err(format!(
                "cannot remove worktree: {}: {}",
                path.display(),
                stderr_msg(&output)
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
            return Err(format!(
                "worktree removed but cannot {action} branch '{branch}': {}",
                stderr_msg(&output)
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
            return Err("cannot prune worktree metadata".into());
        }
        Ok(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }

    pub fn is_dirty(&self, worktree_path: &Path) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(worktree_path)
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
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn rev_resolves(&self, refname: &str) -> bool {
        self.cmd()
            .args(["rev-parse", "--verify", "--quiet", refname])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    pub fn ahead_behind(&self, branch: &str) -> Option<(u64, u64)> {
        let output = self
            .cmd()
            .args([
                "rev-list",
                "--left-right",
                "--count",
                &format!("{branch}@{{upstream}}...{branch}"),
            ])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut parts = text.trim().split('\t');
        let behind: u64 = parts.next()?.parse().ok()?;
        let ahead: u64 = parts.next()?.parse().ok()?;
        Some((ahead, behind))
    }

    pub fn is_upstream_gone(&self, branch: &str) -> bool {
        let branch_ref = format!("refs/heads/{branch}");
        self.upstream_for(&branch_ref).is_some_and(|upstream| {
            upstream.starts_with("origin/") && !self.rev_resolves(&upstream)
        })
    }

    fn upstream_for(&self, refspec: &str) -> Option<String> {
        let output = self
            .cmd()
            .args(["for-each-ref", "--format=%(upstream:short)", refspec])
            .output()
            .ok()?;
        let upstream = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if upstream.is_empty() {
            None
        } else {
            Some(upstream)
        }
    }
}
