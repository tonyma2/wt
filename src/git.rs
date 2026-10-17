use std::collections::BTreeSet;
use std::ffi::OsStr;
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

fn spawn_err(e: std::io::Error) -> String {
    format!("cannot run git: {e}")
}

fn git_in<I, S>(dir: &Path, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    cmd
}

fn run(mut cmd: Command, context: impl AsRef<str>) -> Result<Output, String> {
    let output = cmd.output().map_err(spawn_err)?;
    if !output.status.success() {
        return Err(git_err(context, &output));
    }
    Ok(output)
}

fn query(mut cmd: Command) -> Option<String> {
    let output = cmd.stderr(Stdio::null()).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !stdout.is_empty()).then_some(stdout)
}

fn succeeds(mut cmd: Command) -> bool {
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn run_streaming(mut cmd: Command, context: impl Into<String>) -> Result<(), String> {
    let status = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map_err(spawn_err)?;
    if !status.success() {
        return Err(context.into());
    }
    Ok(())
}

pub fn branch_name(full_ref: &str) -> Option<&str> {
    full_ref.strip_prefix("refs/heads/").or_else(|| {
        full_ref
            .strip_prefix("refs/remotes/")?
            .split_once('/')
            .map(|(_, branch)| branch)
    })
}

pub struct Git {
    repo: PathBuf,
}

impl Git {
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        Self { repo: repo.into() }
    }

    fn cmd<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        git_in(&self.repo, args)
    }

    pub fn find_repo(path: Option<&Path>) -> Result<PathBuf, String> {
        let git = |args: &[&str]| {
            let mut cmd = Command::new("git");
            if let Some(p) = path {
                cmd.arg("-C").arg(p);
            }
            cmd.args(args);
            cmd
        };
        let output = git(&["rev-parse", "--show-toplevel"])
            .stderr(Stdio::null())
            .output()
            .map_err(spawn_err)?;
        if output.status.success() {
            let toplevel = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            return Ok(PathBuf::from(toplevel));
        }
        if let Some(out) = query(git(&[
            "rev-parse",
            "--is-bare-repository",
            "--absolute-git-dir",
        ])) && let Some(git_dir) = out.strip_prefix("true\n")
        {
            return Ok(PathBuf::from(git_dir));
        }
        Err(match path {
            Some(p) => format!("not a git repository: {}", p.display()),
            None => "not a git repository, run inside one".into(),
        })
    }

    pub fn init_bare(dest: &Path) -> Result<(), String> {
        let mut cmd = Command::new("git");
        cmd.args(["init", "--bare", "--quiet"]).arg(dest);
        run(cmd, format!("cannot create repository {}", dest.display())).map(drop)
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<(), String> {
        run(
            self.cmd(["remote", "add", "--", name, url]),
            format!("cannot add remote '{name}'"),
        )
        .map(drop)
    }

    pub fn fetch_remote(&self, remote: &str) -> Result<(), String> {
        run_streaming(
            self.cmd(["fetch", "--prune", remote]),
            format!("cannot fetch from '{remote}'"),
        )
    }

    pub fn remote_url(&self, remote: &str) -> Option<String> {
        query(self.cmd(["remote", "get-url", remote]))
    }

    pub fn base_ref(&self) -> Result<String, String> {
        if let Some(head_ref) =
            query(self.cmd(["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]))
            && let Some(branch) = head_ref.strip_prefix("refs/remotes/origin/")
            && self.ref_exists(&head_ref)
        {
            return Ok(format!("origin/{branch}"));
        }

        ["main", "master"]
            .into_iter()
            .find(|name| self.ref_exists(&format!("refs/remotes/origin/{name}")))
            .map(|name| format!("origin/{name}"))
            .ok_or_else(|| {
                "cannot determine default branch (tried origin/HEAD, origin/main, origin/master)"
                    .into()
            })
    }

    pub fn ref_exists(&self, refname: &str) -> bool {
        succeeds(self.cmd(["show-ref", "--verify", "--quiet", refname]))
    }

    pub fn has_local_branch(&self, name: &str) -> bool {
        self.ref_exists(&format!("refs/heads/{name}"))
    }

    pub fn branch_names(&self) -> BTreeSet<String> {
        query(self.cmd([
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads/",
            "refs/remotes/",
        ]))
        .map(|out| {
            out.lines()
                .filter_map(branch_name)
                .filter(|name| *name != "HEAD")
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
    }

    // Matched per remote: a `refs/remotes/*/{name}` pattern misses remotes whose name has a `/`.
    pub fn remotes_with_branch(&self, name: &str) -> Result<Vec<String>, String> {
        if name == "HEAD" {
            return Ok(vec![]);
        }
        let output = run(self.cmd(["remote"]), "cannot list remotes")?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|remote| self.ref_exists(&format!("refs/remotes/{remote}/{name}")))
            .map(str::to_owned)
            .collect())
    }

    pub fn add_worktree(
        &self,
        branch: &str,
        dest: &Path,
        start: Option<&str>,
        track: bool,
    ) -> Result<(), String> {
        let track = if track { "--track" } else { "--no-track" };
        let mut cmd = self.cmd(["worktree", "add", "--quiet", track, "-b", branch, "--"]);
        cmd.arg(dest).args(start);
        run(cmd, "cannot create worktree").map(drop)
    }

    pub fn checkout_worktree(&self, name: &str, dest: &Path) -> Result<(), String> {
        let mut cmd = self.cmd(["worktree", "add", "--quiet", "--"]);
        cmd.arg(dest).arg(name);
        run(cmd, "cannot create worktree").map(drop)
    }

    pub fn track_branch(&self, branch: &str, upstream: &str) -> Result<(), String> {
        run(
            self.cmd(["branch", "--quiet", "--track", branch, upstream]),
            format!("cannot create branch '{branch}'"),
        )
        .map(drop)
    }

    pub fn set_head(&self, branch: &str) -> Result<(), String> {
        run(
            self.cmd(["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")]),
            "cannot set HEAD",
        )
        .map(drop)
    }

    pub fn head_ref(&self) -> Option<String> {
        query(self.cmd(["symbolic-ref", "--quiet", "HEAD"]))
    }

    pub fn list_worktrees(&self) -> Result<String, String> {
        let output = run(
            self.cmd(["worktree", "list", "--porcelain"]),
            "cannot list worktrees",
        )?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<(), String> {
        let mut cmd = self.cmd(["worktree", "remove"]);
        if force {
            cmd.args(["--force", "--force"]);
        }
        cmd.arg(path);
        run(cmd, format!("cannot remove worktree: {}", path.display())).map(drop)
    }

    pub fn delete_branch(&self, branch: &str) -> Result<(), String> {
        run(
            self.cmd(["branch", "-D", "--quiet", branch]),
            format!("worktree removed but cannot delete branch '{branch}'"),
        )
        .map(drop)
    }

    pub fn prune_worktrees(&self) -> Result<(), String> {
        run(
            self.cmd(["worktree", "prune"]),
            "cannot prune worktree metadata",
        )
        .map(drop)
    }

    pub fn worktree_status(worktree_path: &Path) -> Option<String> {
        let output = git_in(
            worktree_path,
            [
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=normal",
            ],
        )
        .stderr(Stdio::null())
        .output()
        .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn tracks(worktree_path: &Path, path: &str) -> Result<bool, String> {
        run(
            git_in(
                worktree_path,
                ["ls-files", "--", &format!(":(literal){path}")],
            ),
            format!("cannot check whether {path} is tracked"),
        )
        .map(|output| !output.stdout.is_empty())
    }

    pub fn is_ignored(worktree_path: &Path, path: &str) -> bool {
        succeeds(git_in(
            worktree_path,
            ["check-ignore", "--quiet", "--", path],
        ))
    }

    // Reachable from the upstream, HEAD, or the remote's default branch, or squash-merged into
    // the default branch. `git branch -d` checks only one of these.
    pub fn is_branch_merged(&self, branch: &str) -> bool {
        let branch_ref = format!("refs/heads/{branch}");
        let base = self.base_ref().ok();
        let upstream = self.upstream(branch);
        upstream
            .iter()
            .chain(&base)
            .map(String::as_str)
            .chain(["HEAD"])
            .any(|target| self.is_ancestor(&branch_ref, target))
            || base.is_some_and(|b| self.is_squash_merged(&branch_ref, &b))
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        succeeds(self.cmd(["merge-base", "--is-ancestor", ancestor, descendant]))
    }

    pub fn rev_parse(&self, refname: &str) -> Option<String> {
        query(self.cmd([
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            refname,
        ]))
    }

    pub fn short_sha(&self, refname: &str) -> Option<String> {
        query(self.cmd([
            "rev-parse",
            "--short",
            "--verify",
            "--quiet",
            "--end-of-options",
            refname,
        ]))
    }

    pub fn is_squash_merged(&self, branch_ref: &str, base: &str) -> bool {
        let merged_tree = query(self.cmd(["merge-tree", "--write-tree", base, branch_ref]));
        merged_tree.is_some_and(|tree| self.rev_parse(&format!("{base}^{{tree}}")) == Some(tree))
    }

    pub fn upstream(&self, branch: &str) -> Option<String> {
        query(self.cmd([
            "for-each-ref",
            "--format=%(upstream)",
            &format!("refs/heads/{branch}"),
        ]))
    }

    pub fn full_ref(&self, name: &str) -> Option<String> {
        query(self.cmd([
            "rev-parse",
            "--symbolic-full-name",
            "--verify",
            "--quiet",
            "--end-of-options",
            name,
        ]))
    }

    pub fn has_refs(&self, prefix: &str) -> bool {
        query(self.cmd(["for-each-ref", "--count=1", prefix])).is_some()
    }

    pub fn common_dir(&self) -> Option<PathBuf> {
        query(self.cmd(["rev-parse", "--path-format=absolute", "--git-common-dir"]))
            .map(PathBuf::from)
    }

    pub fn set_remote_head(&self, remote: &str) -> Result<(), String> {
        run(
            self.cmd(["remote", "set-head", remote, "--auto"]),
            format!("cannot detect default branch from '{remote}'"),
        )
        .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
