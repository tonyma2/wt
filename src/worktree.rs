use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Default)]
struct PorcelainParser {
    path: Option<PathBuf>,
    head: String,
    branch: Option<String>,
    bare: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
}

impl PorcelainParser {
    fn flush(&mut self, out: &mut Vec<Worktree>) {
        if let Some(path) = self.path.take() {
            out.push(Worktree {
                path,
                head: std::mem::take(&mut self.head),
                branch: self.branch.take(),
                bare: self.bare,
                detached: self.detached,
                locked: self.locked,
                prunable: self.prunable,
            });
            self.bare = false;
            self.detached = false;
            self.locked = false;
            self.prunable = false;
        }
    }
}

pub fn parse_porcelain(output: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut parser = PorcelainParser::default();

    for line in output.lines() {
        if line.is_empty() {
            parser.flush(&mut worktrees);
        } else if let Some(rest) = line.strip_prefix("worktree ") {
            parser.flush(&mut worktrees);
            parser.path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            parser.head = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let short = rest.strip_prefix("refs/heads/").unwrap_or(rest);
            parser.branch = Some(short.to_string());
        } else if line == "bare" {
            parser.bare = true;
        } else if line == "detached" {
            parser.detached = true;
        } else if line == "locked" || line.starts_with("locked ") {
            parser.locked = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            parser.prunable = true;
        }
    }

    parser.flush(&mut worktrees);
    worktrees
}

pub fn find_by_branch<'a>(worktrees: &'a [Worktree], name: &str) -> Vec<&'a Worktree> {
    worktrees
        .iter()
        .filter(|wt| wt.branch.as_deref() == Some(name))
        .collect()
}

pub fn find_by_path<'a>(worktrees: &'a [Worktree], path: &Path) -> Option<&'a Worktree> {
    worktrees.iter().find(|wt| wt.path == path)
}

pub fn branch_checked_out_elsewhere(
    worktrees: &[Worktree],
    branch: &str,
    exclude_path: &Path,
) -> bool {
    worktrees
        .iter()
        .any(|wt| wt.branch.as_deref() == Some(branch) && wt.path != exclude_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_worktree() {
        let input = "\
worktree /home/user/project
HEAD abc123def456
branch refs/heads/main
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].path, PathBuf::from("/home/user/project"));
        assert_eq!(wts[0].head, "abc123def456");
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert!(!wts[0].bare);
        assert!(!wts[0].detached);
        assert!(!wts[0].locked);
        assert!(!wts[0].prunable);
    }

    #[test]
    fn bare_worktree() {
        let input = "\
worktree /home/user/project.git
HEAD 0000000000000000000000000000000000000000
bare
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert!(wts[0].bare);
        assert!(wts[0].branch.is_none());
    }

    #[test]
    fn detached_head() {
        let input = "\
worktree /home/user/project
HEAD abc123
detached
";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 1);
        assert!(wts[0].detached);
        assert!(wts[0].branch.is_none());
    }

    #[test]
    fn multiple_worktrees() {
        let input = "\
worktree /home/user/project
HEAD abc123
branch refs/heads/main

worktree /home/user/.worktrees/project/feature
HEAD def456
branch refs/heads/feature
locked

";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 2);
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert!(!wts[0].locked);
        assert_eq!(wts[1].branch.as_deref(), Some("feature"));
        assert!(wts[1].locked);
    }

    #[test]
    fn no_trailing_blank_line() {
        let input = "\
worktree /home/user/project
HEAD abc123
branch refs/heads/main

worktree /home/user/.worktrees/project/feature
HEAD def456
branch refs/heads/feature";
        let wts = parse_porcelain(input);
        assert_eq!(wts.len(), 2);
        assert_eq!(wts[1].branch.as_deref(), Some("feature"));
    }
}
