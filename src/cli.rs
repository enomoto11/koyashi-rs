//! Command-line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// koyashi — detect freeloader struct fields in a Rust workspace.
#[derive(Debug, Parser)]
#[command(name = "koyashi", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Scan a workspace and report freeloader fields.
    Check(CheckArgs),
}

/// Arguments for `koyashi check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Workspace or crate root to analyze.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Restrict output to the given classifications (comma-separated).
    #[arg(long, value_delimiter = ',')]
    pub include: Vec<ClassFilter>,

    /// Exclude `#[cfg(test)]` modules from reference counting.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub exclude_tests: bool,

    /// Restrict analysis to a single struct.
    #[arg(long = "struct")]
    pub struct_name: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Severity threshold that controls the exit code.
    #[arg(long, value_enum, default_value_t = Severity::Warn)]
    pub severity: Severity,
}

/// A classification accepted by `--include`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ClassFilter {
    Unused,
    WriteOnly,
    ReadOnly,
    DeriveOnly,
}

/// Output format for `check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Severity threshold for the process exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Severity {
    Info,
    Warn,
    Error,
}
