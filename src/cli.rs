//! Command-line interface definitions for recallwell.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "recallwell",
    version,
    about = "Your knowledge, indexed and citable",
    long_about = "recallwell is a personal knowledge base built on pagebridge. \
                  Ingest documents, ask questions, get cited answers, all on your machine."
)]
pub struct Cli {
    /// Override the data directory (default: OS-standard data dir).
    #[arg(long, global = true, value_name = "PATH")]
    pub data_dir: Option<PathBuf>,

    /// Override the config file path.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Enable verbose logging.
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start the recallwell server (default if no subcommand is given).
    Serve {
        /// Port to bind to (default 7676, or RECALLWELL_PORT env).
        #[arg(long, env = "RECALLWELL_PORT")]
        port: Option<u16>,

        /// Open the browser automatically.
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        auto_open: bool,
    },

    /// Run the first-time setup wizard.
    Setup,

    /// Show the configuration path and current (redacted) values.
    Config {
        /// Open the config file in $EDITOR.
        #[arg(long)]
        edit: bool,
    },

    /// List libraries known to recallwell.
    Libraries,

    /// Print the recallwell version and exit.
    Version,
}
