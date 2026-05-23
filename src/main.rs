mod cli;
mod config;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Command};

fn main() -> Result<()> {
    let args = Cli::parse();
    match args.command.unwrap_or(Command::Serve {
        port: None,
        auto_open: true,
    }) {
        Command::Serve { .. } => {
            println!("recallwell v{} (serve not yet wired)", env!("CARGO_PKG_VERSION"));
        }
        Command::Setup => {
            println!("setup wizard not yet wired");
        }
        Command::Config { .. } => {
            println!("config command not yet wired");
        }
        Command::Libraries => {
            println!("libraries command not yet wired");
        }
        Command::Version => {
            println!("recallwell v{}", env!("CARGO_PKG_VERSION"));
        }
    }
    Ok(())
}
