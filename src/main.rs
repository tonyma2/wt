mod cli;
mod commands;
mod fuzzy;
mod git;
mod layout;
mod terminal;
mod tui;
mod worktree;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};

unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

fn main() -> ExitCode {
    // SIGPIPE (13) back to SIG_DFL (0): Rust ignores it, so a closed pipe made println! panic
    unsafe {
        signal(13, 0);
    }
    let cli = Cli::parse();
    let result = match cli.command {
        None => commands::tui::run(),
        Some(Command::Clone { url }) => commands::clone::run(&url),
        Some(Command::Init { shell }) => commands::init::run(shell),
        Some(Command::Switch { name, base, create }) => {
            commands::switch::run(&name, base.as_deref(), create)
        }
        Some(Command::List { all }) => commands::list::run(all),
        Some(Command::Remove { names, force }) => commands::rm::run(&names, force),
        Some(Command::Prune { dry_run }) => commands::prune::run(dry_run),
        Some(Command::Link { files, force }) => commands::link::run(&files, force),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
