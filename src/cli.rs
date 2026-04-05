use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wt", version, about = "Git worktree manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a worktree for a branch or ref
    #[command(
        visible_alias = "n",
        long_about = "Create a worktree for a branch or ref.\n\
            By default, checks out an existing branch or ref.\n\
            Use --create to create a new branch from HEAD, or provide [base] to create from a specific start point.\n\
            Tags and other non-branch refs check out as detached HEAD.\n\
            Worktrees are created under ~/.wt/worktrees/<id>/<repo>/.",
        after_help = "Examples:\n  wt new feat/login\n  wt new -c feat/login\n  wt new -c feat/login develop\n  wt new fix/session-timeout --repo /path/to/repo\n  wt new v1.0"
    )]
    New {
        /// Branch name or ref
        name: String,
        /// Create a new branch instead of checking out an existing ref
        #[arg(short = 'c', long = "create")]
        create: bool,
        /// Start point for created branch (requires --create)
        #[arg(requires = "create")]
        base: Option<String>,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// List worktrees
    #[command(
        visible_alias = "ls",
        long_about = "List worktrees for the current repository.\n\
            The leading '*' marks the active/current worktree.\n\
            Use --all to list worktrees across all repositories managed under ~/.wt/worktrees/.",
        after_help = "Examples:\n  wt ls\n  wt ls --repo /path/to/repo\n  wt ls --json\n  wt ls --all\n  wt ls --all --json"
    )]
    List {
        /// Repository path
        #[arg(long, conflicts_with = "all")]
        repo: Option<PathBuf>,
        /// Output as JSON array
        #[arg(long)]
        json: bool,
        /// List worktrees across all discovered repositories
        #[arg(long)]
        all: bool,
    },
    /// Remove worktrees by name, ref, or path
    #[command(
        visible_alias = "rm",
        long_about = "Remove worktrees by branch name, ref, or worktree root path.\n\
            Tags and other non-branch refs are resolved to detached HEAD worktrees.\n\
            Name lookup requires repository context (current repo or --repo).\n\
            Also deletes the local branch by default.\n\
            Use --force to remove dirty worktrees and force-delete the branch.",
        after_help = "Examples:\n  wt rm feat/login\n  wt rm v1.0\n  wt rm feat/a feat/b feat/c\n  wt rm /Users/me/.wt/worktrees/a3f2/my-repo\n  wt rm feat/login --force"
    )]
    Remove {
        /// Branch names, refs, or paths
        #[arg(required = true)]
        names: Vec<String>,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Force removal
        #[arg(long)]
        force: bool,
        /// Remove the worktree but keep the branch
        #[arg(long)]
        keep_branch: bool,
    },
    /// Clean up merged, stale, and orphaned worktrees
    #[command(
        long_about = "Clean up merged, stale, and orphaned worktrees.\n\n\
            Removes worktrees whose branch is fully merged into the base branch. \
            Also prunes stale worktree metadata for missing directories, and removes \
            orphaned worktree directories whose backing repository has been deleted.\n\n\
            Use --gone to also remove worktrees whose upstream tracking branch no longer \
            exists (e.g. after a squash-merge deleted the remote branch).\n\n\
            Use --base to override the auto-detected default branch for merged detection \
            (useful when the base branch is not main/master, or there is no remote).\n\n\
            By default, discovers all repos from ~/.wt/worktrees/ and prunes each one. \
            Use --repo to target a single repository.",
        after_help = "Examples:\n  wt prune\n  wt prune --gone\n  wt prune --base develop\n  wt prune --dry-run\n  wt prune --repo /path/to/repo"
    )]
    Prune {
        /// Show what would be done without doing it
        #[arg(long, short = 'n')]
        dry_run: bool,
        /// Also remove worktrees whose upstream branch is gone
        #[arg(long)]
        gone: bool,
        /// Repository path (prune only this repo, skip orphan cleanup)
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Base branch for merged detection (e.g. develop, trunk)
        #[arg(long)]
        base: Option<String>,
    },
    /// Set up shell integration (completions + directory switching)
    #[command(
        long_about = "Set up shell integration.\n\
            Outputs completions and a wrapper function that auto-changes \
            directory after new and switch.",
        after_help = "Examples:\n  eval \"$(wt init zsh)\"\n  eval \"$(wt init bash)\"\n  wt init fish | source"
    )]
    Init {
        /// Shell to generate integration for
        shell: clap_complete::Shell,
    },
    /// Print the path to a worktree
    #[command(
        visible_alias = "p",
        long_about = "Print the path to a worktree.\n\
            Looks up by branch name. Tags and other non-branch refs are resolved \
            to a commit SHA and matched against detached HEAD worktrees.",
        after_help = "Examples:\n  wt path feat/login\n  wt path v1.0\n  cd \"$(wt p feat/login)\""
    )]
    Path {
        /// Branch name, tag, or ref
        name: String,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Switch to a worktree, creating one if needed
    #[command(
        visible_alias = "s",
        long_about = "Switch to a worktree, creating one if needed.\n\
            If a worktree already exists for the branch, prints its path.\n\
            If the branch exists (local or remote) but has no worktree, checks it out into a new one.\n\
            If no branch with this name exists and no similar branch exists, creates one from HEAD.\n\
            If a similar branch name exists (possible typo), errors with a suggestion.\n\
            Use --create to skip the typo check and force creation.\n\
            Non-branch refs (tags, SHAs) are rejected; use `wt new` instead.",
        after_help = "Examples:\n  wt switch feat/login\n  wt s feat/login\n  wt switch -c feat/new-branch\n  cd \"$(wt switch feat/login)\""
    )]
    Switch {
        /// Branch name
        name: String,
        /// Create a new branch, skipping the similar-name check
        #[arg(short = 'c', long = "create")]
        create: bool,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Link files from the primary worktree into linked worktrees
    #[command(
        visible_alias = "ln",
        long_about = "Link files from the primary worktree into all linked worktrees.\n\
            Source files must exist in the primary worktree.\n\
            Correct symlinks are left in place. Conflicts are skipped unless --force is used.",
        after_help = "Examples:\n  wt link .env .env.local\n  wt link config/.env\n  wt link .env --force\n  wt link --list"
    )]
    Link {
        /// Files or directories to link
        #[arg(required_unless_present = "list", conflicts_with = "list")]
        files: Vec<String>,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Replace existing destinations that are not correct symlinks
        #[arg(long)]
        force: bool,
        /// List currently configured links for this repository
        #[arg(long)]
        list: bool,
    },
    /// Clone a repository and create the first worktree
    #[command(
        visible_alias = "cl",
        long_about = "Clone a repository and create the first worktree.\n\
            The repository is stored as a bare clone under ~/.wt/repos/.\n\
            A worktree for the default branch is created under ~/.wt/worktrees/.",
        after_help = "Examples:\n  wt clone git@github.com:org/repo.git\n  wt clone https://github.com/org/repo"
    )]
    Clone {
        /// Repository URL
        url: String,
    },
    /// Remove linked files from linked worktrees
    #[command(
        long_about = "Remove previously linked files from all linked worktrees.\n\
            Only removes symlinks that point back to the primary worktree.\n\
            Non-symlinks and symlinks pointing elsewhere are skipped unless --force is used.\n\
            Use --all to unlink all previously linked files.",
        after_help = "Examples:\n  wt unlink .env\n  wt unlink .env .env.local\n  wt unlink .env --force\n  wt unlink --all"
    )]
    Unlink {
        /// Files or directories to unlink
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        files: Vec<String>,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Remove even if not a symlink to the primary worktree
        #[arg(long)]
        force: bool,
        /// Unlink all previously linked files
        #[arg(long)]
        all: bool,
    },
}
