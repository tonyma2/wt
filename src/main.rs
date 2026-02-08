mod cli;
mod commands;
mod git;
mod terminal;
mod worktree;

use clap::Parser;
use cli::{Cli, Command};
use std::process;

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Completions { shell } => commands::completions::run(*shell),
        Command::New { name, repo } => commands::new::run(name, repo.as_deref()),
        Command::List { repo, porcelain } => commands::list::run(repo.as_deref(), *porcelain),
        Command::Remove { names, repo, force } => commands::rm::run(names, repo.as_deref(), *force),
        Command::Prune { dry_run, repo } => commands::prune::run(*dry_run, repo.as_deref()),
        Command::Path { name, repo } => commands::path::run(name, repo.as_deref()),
        Command::Link { files, repo, force } => commands::link::run(files, repo.as_deref(), *force),
    };

    if let Err(e) = result {
        eprintln!("wt: {e}");
        process::exit(1);
    }
}
