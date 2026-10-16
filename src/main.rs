mod cli;
mod commands;
mod fuzzy;
mod git;
mod layout;
mod links;
mod terminal;
mod tui;
mod worktree;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    links::migrate_legacy_config();
    let result = match cli.command {
        None => commands::tui::run(),
        Some(Command::Clone { url }) => commands::clone::run(&url),
        Some(Command::Init { shell }) => commands::init::run(shell),
        Some(Command::Switch {
            name,
            base,
            create,
            repo,
        }) => commands::switch::run(&name, base.as_deref(), create, repo.as_deref()),
        Some(Command::List { repo, all }) => commands::list::run(repo.as_deref(), all),
        Some(Command::Remove {
            names,
            repo,
            force,
            keep_branch,
        }) => commands::rm::run(&names, repo.as_deref(), force, keep_branch),
        Some(Command::Prune(args)) => commands::prune::run(&args),
        Some(Command::Path { name, repo }) => commands::path::run(&name, repo.as_deref()),
        Some(Command::Link {
            files,
            repo,
            force,
            list,
        }) => commands::link::run(&files, repo.as_deref(), force, list),
        Some(Command::Unlink { files, repo, all }) => {
            commands::unlink::run(&files, repo.as_deref(), all)
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
