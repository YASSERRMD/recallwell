use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use recallwell::cli::{Cli, Command};
use recallwell::commands;
use recallwell::config::{CliOverrides, Config};
use recallwell::server;

fn main() -> Result<()> {
    let args = Cli::parse();
    init_tracing(args.verbose);

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
            let config = Config::load(&overrides)?;
            config.validate()?;
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(server::run(Arc::new(config)))?;
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

fn init_tracing(verbose: bool) {
    let default = if verbose { "debug,recallwell=trace" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
