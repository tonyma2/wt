use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "wt",
    version,
    about = "Git worktree manager",
    after_help = "Run `wt` without a command to pick a worktree interactively."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Switch to a worktree, creating one if needed
    #[command(
        visible_alias = "s",
        long_about = "Switch to a worktree, creating one if needed.\n\
            If a worktree already exists for the branch, prints its path.\n\
            If the branch exists (local or remote) but has no worktree, checks it out into a new one.\n\
            If no branch with this name exists and no similar branch exists, creates one from HEAD.\n\
            If a similar branch name exists (possible typo), errors with a suggestion.\n\
            Use --create to skip the typo check and force creation.\n\
            Provide [base] to create the branch from a specific start point (branch, tag, or commit).\n\
            Worktrees are created under ~/.wt/worktrees/<id>/<repo>/.",
        after_help = "Examples:\n  wt switch feat/login\n  wt s feat/login\n  wt switch -c feat/logn\n  wt switch feat/login develop\n  wt switch hotfix v1.0\n  wt switch feat/login --repo /path/to/repo\n  cd \"$(wt switch feat/login)\""
    )]
    Switch {
        /// Branch name
        name: String,
        /// Start point for a new branch
        base: Option<String>,
        /// Create a new branch, skipping the similar-name check
        #[arg(short = 'c', long = "create")]
        create: bool,
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
        after_help = "Examples:\n  wt ls\n  wt ls --repo /path/to/repo\n  wt ls --all"
    )]
    List {
        /// Repository path
        #[arg(long, conflicts_with = "all")]
        repo: Option<PathBuf>,
        /// List worktrees across all discovered repositories
        #[arg(long)]
        all: bool,
    },
    /// Remove worktrees by branch or path
    #[command(
        visible_alias = "rm",
        long_about = "Remove worktrees by branch name or worktree root path.\n\
            Name lookup requires repository context (current repo or --repo).\n\
            Also deletes the local branch by default.\n\
            Use --force to remove dirty worktrees and force-delete the branch.",
        after_help = "Examples:\n  wt rm feat/login\n  wt rm feat/a feat/b feat/c\n  wt rm /Users/me/.wt/worktrees/a3f2/my-repo\n  wt rm feat/login --force"
    )]
    Remove {
        /// Branch names or worktree paths
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
            Removes clean worktrees whose branch is fully merged into the base branch, \
            along with the branch. The base branch, the remote's default branch, the \
            repo's HEAD branch, and the worktree that is the source for `wt link` are \
            never removed. Also repairs worktree directories that were renamed, prunes \
            stale worktree metadata, and removes bare clones under ~/.wt/repos/ that have \
            no worktrees and no unpushed commits.\n\n\
            Use --gone to also remove worktrees whose upstream branch no longer exists, \
            as long as their changes landed in the base (e.g. after a squash-merge \
            deleted the remote branch).\n\n\
            Use --stale to also remove worktrees whose branch was never pushed. \
            Commits on those branches that never reached the base are lost.\n\n\
            Use --orphans to also delete worktree directories whose repository no longer \
            exists. Their contents, including uncommitted files, are lost.\n\n\
            Use --base to override the auto-detected default branch for merged detection \
            (useful when the base branch is not main/master, or there is no remote). \
            With --base, branches that were never pushed also count as merged, including \
            new branches with no commits of their own.\n\n\
            By default, discovers all repos from ~/.wt/worktrees/ and prunes each one. \
            Use --repo to target a single repository.",
        after_help = "Examples:\n  wt prune\n  wt prune --gone\n  wt prune --stale\n  wt prune --orphans\n  wt prune --base develop\n  wt prune --dry-run\n  wt prune --repo /path/to/repo"
    )]
    Prune(PruneArgs),
    /// Set up shell integration (completions + directory switching)
    #[command(
        long_about = "Set up shell integration.\n\
            Outputs completions and a wrapper function that auto-changes \
            directory after switch and clone.",
        after_help = "Examples:\n  eval \"$(wt init zsh)\"\n  eval \"$(wt init bash)\"\n  wt init fish | source"
    )]
    Init {
        /// Shell to generate integration for
        shell: clap_complete::Shell,
    },
    /// Print the path to a worktree
    #[command(
        visible_alias = "p",
        long_about = "Print the path to the worktree that has a branch checked out.",
        after_help = "Examples:\n  wt path feat/login\n  cd \"$(wt p feat/login)\""
    )]
    Path {
        /// Branch checked out in the worktree
        name: String,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Share untracked files across all worktrees of a repository
    #[command(
        visible_alias = "ln",
        long_about = "Share untracked files (e.g. .env) across all worktrees of a repository.\n\
            The file moves from the current worktree into the repository's git directory \
            (.git/wt/links/) and every worktree, including the current one, gets a symlink to it, \
            so removing any worktree never removes the file. New worktrees are linked automatically.\n\
            Running it again for a shared file repairs missing links. \
            Existing files in other worktrees are skipped unless --force is used.",
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
    /// Stop sharing files, leaving a copy in each worktree
    #[command(
        long_about = "Stop sharing previously linked files.\n\
            Each worktree's symlink is replaced with a copy of the file, then the shared copy is removed. \
            Anything that is not a symlink to the shared file is skipped.\n\
            Use --all to stop sharing every linked file.",
        after_help = "Examples:\n  wt unlink .env\n  wt unlink .env .env.local\n  wt unlink --all"
    )]
    Unlink {
        /// Files or directories to unlink
        #[arg(required_unless_present = "all", conflicts_with = "all")]
        files: Vec<String>,
        /// Repository path
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Unlink all previously linked files
        #[arg(long)]
        all: bool,
    },
}

#[derive(Args)]
pub struct PruneArgs {
    /// Show what would be done without doing it
    #[arg(long, short = 'n')]
    pub dry_run: bool,
    /// Also remove worktrees whose upstream branch is gone and whose changes landed in the base
    #[arg(long)]
    pub gone: bool,
    /// Also remove worktrees whose branch was never pushed
    #[arg(long)]
    pub stale: bool,
    /// Also delete orphaned worktree directories whose repository is gone
    #[arg(long)]
    pub orphans: bool,
    /// Repository path (prune only this repo, skip orphan cleanup)
    #[arg(long)]
    pub repo: Option<PathBuf>,
    /// Base branch for merged detection (e.g. develop, trunk)
    #[arg(long)]
    pub base: Option<String>,
}
