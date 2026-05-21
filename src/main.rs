//! koyashi — a CLI that detects freeloader struct fields in Rust workspaces.

mod analysis;
mod cli;
mod commands;
mod lsp;
mod model;
mod report;
mod rust_analyzer;
mod source;
mod workspace;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    match commands::run(cli) {
        Ok(exit_code) => ExitCode::from(exit_code),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}
