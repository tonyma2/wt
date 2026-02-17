use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "wt", version, about = "Git worktree manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
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
            The leading '*' marks the active/current worktree.",
        after_help = "Examples:\n  wt ls\n  wt ls --repo /path/to/repo\n  wt ls --porcelain"
    )]
    List {
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Machine-readable output
        #[arg(long)]
        porcelain: bool,
    },
    /// Remove worktrees by name or path
    #[command(
        visible_alias = "rm",
        long_about = "Remove linked worktrees by branch name or exact worktree root path.\n\
            Name lookup requires repository context (current repo or --repo).\n\
            Also deletes the linked local branch by default.\n\
            Use --force to remove dirty worktrees and force-delete the branch.",
        after_help = "Examples:\n  wt rm feat/login\n  wt rm feat/a feat/b feat/c\n  wt rm /Users/me/.wt/worktrees/a3f2/my-repo\n  wt rm feat/login --force"
    )]
    Remove {
        /// Branch names or paths
        #[arg(required = true)]
        names: Vec<String>,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Force removal
        #[arg(long)]
        force: bool,
    },
    /// Clean up stale worktree metadata and orphaned directories
    #[command(
        long_about = "Remove stale worktree metadata for missing directories, \
            and remove orphaned worktree directories whose backing repository \
            has been deleted.\n\n\
            Worktrees whose branch is fully merged into the base branch are also removed.\n\n\
            Use --gone to also remove worktrees whose upstream tracking branch no longer \
            exists (e.g. after a squash-merge deleted the remote branch).\n\n\
            By default, discovers all repos from ~/.wt/worktrees/ and prunes each one, \
            then cleans up orphaned directories. Use --repo to target a single repository.",
        after_help = "Examples:\n  wt prune\n  wt prune --gone\n  wt prune --dry-run\n  wt prune --repo /path/to/repo"
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
    },
    /// Generate shell completions
    #[command(
        long_about = "Generate shell completion scripts.\n\
            Add to your shell configuration to enable tab completion.",
        after_help = "Examples:\n  eval \"$(wt completions zsh)\"\n  eval \"$(wt completions bash)\"\n  wt completions fish | source"
    )]
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
    /// Print the path to a worktree
    #[command(
        visible_alias = "p",
        after_help = "Examples:\n  wt path feat/login\n  cd \"$(wt p feat/login)\""
    )]
    Path {
        /// Worktree branch name
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
            If no branch with this name exists, creates one from HEAD.\n\
            Non-branch refs (tags, SHAs) are rejected; use `wt new` instead.",
        after_help = "Examples:\n  wt switch feat/login\n  wt s feat/login\n  cd \"$(wt switch feat/login)\""
    )]
    Switch {
        /// Worktree branch name
        name: String,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Link files from the primary worktree into linked worktrees
    #[command(
        visible_alias = "ln",
        long_about = "Link files from the primary worktree into all linked worktrees.\n\
            Source files must exist in the primary worktree.\n\
            Correct symlinks are skipped. Non-symlink conflicts warn and skip unless --force is used.",
        after_help = "Examples:\n  wt link .env .env.local\n  wt link config/.env\n  wt link .env --force"
    )]
    Link {
        /// Files or directories to link
        #[arg(required = true)]
        files: Vec<String>,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Replace existing destinations that are not correct symlinks
        #[arg(long)]
        force: bool,
    },
}
