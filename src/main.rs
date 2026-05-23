mod cli;
mod commands;
mod config;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Command};
use crate::config::CliOverrides;

fn main() -> Result<()> {
    let args = Cli::parse();
    let overrides = CliOverrides {
        data_dir: args.data_dir.clone(),
        config_path: args.config.clone(),
        ..Default::default()
    };

    let command = args.command.unwrap_or(Command::Serve {
        port: None,
        auto_open: true,
    });

    match command {
        Command::Serve { port, auto_open } => {
            let overrides = CliOverrides {
                port,
                auto_open: Some(auto_open),
                ..overrides
            };
            println!(
                "recallwell v{} (serve not yet wired; overrides = {overrides:?})",
                env!("CARGO_PKG_VERSION")
            );
        }
        Command::Setup => commands::run_setup(&overrides)?,
        Command::Config { edit } => commands::run_config(&overrides, edit)?,
        Command::Libraries => commands::run_libraries(&overrides)?,
        Command::Version => {
            println!("recallwell v{}", env!("CARGO_PKG_VERSION"));
        }
    }
    Ok(())
}
