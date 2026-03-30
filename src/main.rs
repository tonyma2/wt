mod cli;
mod commands;
mod config;
mod fuzzy;
mod git;
mod terminal;
mod tui;
mod worktree;

use std::process;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        None => commands::tui::run(),
        Some(Command::Clone { url }) => commands::clone::run(url),
        Some(Command::Init { shell }) => commands::init::run(*shell),
        Some(Command::New {
            name,
            create,
            base,
            repo,
        }) => commands::new::run(name, *create, base.as_deref(), repo.as_deref()),
        Some(Command::List { repo, json, all }) => {
            commands::list::run(repo.as_deref(), *json, *all)
        }
        Some(Command::Remove {
            names,
            repo,
            force,
            keep_branch,
        }) => commands::rm::run(names, repo.as_deref(), *force, *keep_branch),
        Some(Command::Prune {
            dry_run,
            gone,
            repo,
            base,
        }) => commands::prune::run(*dry_run, *gone, repo.as_deref(), base.as_deref()),
        Some(Command::Path { name, repo }) => commands::path::run(name, repo.as_deref()),
        Some(Command::Switch { name, create, repo }) => {
            commands::switch::run(name, *create, repo.as_deref())
        }
        Some(Command::Link {
            files,
            repo,
            force,
            list,
        }) => commands::link::run(files, repo.as_deref(), *force, *list),
        Some(Command::Unlink {
            files,
            repo,
            force,
            all,
        }) => commands::unlink::run(files, repo.as_deref(), *force, *all),
    };

    if let Err(e) = result {
        eprintln!("{e}");
        process::exit(1);
    }
}
