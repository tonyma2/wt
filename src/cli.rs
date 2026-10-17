use clap::{Parser, Subcommand};

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
        after_help = "Examples:\n  wt switch feat/login\n  wt s feat/login\n  wt switch -c feat/logn\n  wt switch feat/login develop\n  wt switch hotfix v1.0\n  cd \"$(wt switch feat/login)\""
    )]
    Switch {
        /// Branch name
        name: String,
        /// Start point for a new branch
        base: Option<String>,
        /// Create a new branch, skipping the similar-name check
        #[arg(short = 'c', long = "create")]
        create: bool,
    },
    /// List worktrees
    #[command(
        visible_alias = "ls",
        long_about = "List worktrees for the current repository.\n\
            The leading '*' marks the current worktree.\n\
            STATUS: '*' uncommitted changes, '↑N' ahead of upstream, '↓N' behind upstream.\n\
            Use --all to list worktrees across all repositories managed under ~/.wt/worktrees/.",
        after_help = "Examples:\n  wt ls\n  wt ls --all"
    )]
    List {
        /// List worktrees across all discovered repositories
        #[arg(short, long)]
        all: bool,
    },
    /// Remove worktrees by branch or path
    #[command(
        visible_alias = "rm",
        long_about = "Remove worktrees by branch name or worktree root path.\n\
            Name lookup requires running inside the repository.\n\
            Also deletes the local branch if it is merged.\n\
            Use --force to remove dirty worktrees and force-delete unmerged branches.",
        after_help = "Examples:\n  wt rm feat/login\n  wt rm feat/a feat/b feat/c\n  wt rm /Users/me/.wt/worktrees/a3f2/my-repo\n  wt rm feat/login --force"
    )]
    Remove {
        /// Branch names or worktree paths
        #[arg(required = true)]
        names: Vec<String>,
        /// Force removal
        #[arg(short, long)]
        force: bool,
    },
    /// Clean up worktrees whose branches have been merged
    #[command(
        long_about = "Clean up worktrees whose branches have been merged.\n\n\
            Fetches origin, then removes each clean worktree whose branch was pushed and \
            whose changes are in the default branch (merged or squash-merged), along with \
            the branch. The default branch and the repo's HEAD branch are never removed, \
            nor are locked worktrees, the current one, or one holding another worktree. \
            Repos without an origin remote are skipped. Also reports worktree directories \
            that are no longer registered with a repository.\n\n\
            Covers every repo with a worktree under ~/.wt/worktrees/.",
        after_help = "Examples:\n  wt prune\n  wt prune --dry-run"
    )]
    Prune {
        /// Show what would be done without doing it
        #[arg(long, short = 'n')]
        dry_run: bool,
    },
    /// Set up shell integration (completions + directory switching)
    #[command(
        long_about = "Set up shell integration.\n\
            Outputs completions and a wrapper function that auto-changes \
            directory after switch and clone. The wrapper is only generated for bash, zsh, and fish.",
        after_help = "Examples:\n  eval \"$(wt init zsh)\"\n  eval \"$(wt init bash)\"\n  wt init fish | source"
    )]
    Init {
        /// Shell to generate integration for
        shell: clap_complete::Shell,
    },
    /// Share untracked files across all worktrees of a repository
    #[command(
        visible_alias = "ln",
        long_about = "Share untracked files (e.g. .env) across all worktrees of a repository.\n\
            The file moves from the current worktree into the repository's git directory \
            (.git/wt/links/) and every worktree, including the current one, gets a symlink to it, \
            so removing any worktree never removes the file. New worktrees are linked automatically.\n\
            Paths are relative to the worktree root. Directories cannot be shared; link the files inside them. \
            Running it again for a shared file repairs missing links. \
            Existing files in other worktrees are skipped unless --force is used.",
        after_help = "Examples:\n  wt link .env .env.local\n  wt link config/.env\n  wt link .env --force"
    )]
    Link {
        /// Files to link
        #[arg(required = true)]
        files: Vec<String>,
        /// Replace existing destinations that are not correct symlinks
        #[arg(short, long)]
        force: bool,
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
}
