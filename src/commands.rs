//! Implementations for the non-serve CLI subcommands.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

use crate::config::{CliOverrides, Config};

/// Interactive first-run setup. Prompts for the Groq API key, writes the
/// config to the OS-standard path, creates the default data directories.
pub fn run_setup(overrides: &CliOverrides) -> Result<()> {
    let mut config = Config::load(overrides).unwrap_or_default();

    println!("recallwell setup");
    println!("================");
    println!();
    println!("Get your Groq API key at https://console.groq.com");
    println!("It will be saved at {}", Config::config_path()?.display());
    println!();

    if !std::io::stdin().is_terminal() {
        return Err(anyhow!(
            "setup requires an interactive terminal (stdin is not a TTY)"
        ));
    }

    print!("Groq API key: ");
    std::io::stdout().flush().ok();
    let mut buf = String::new();
    std::io::stdin().lock().read_line(&mut buf)?;
    let key = buf.trim().to_string();
    if key.is_empty() {
        return Err(anyhow!("API key is required"));
    }
    config.groq.api_key = Some(key);

    let saved = config.save()?;
    println!();
    println!("Wrote config to {}", saved.display());

    config.validate()?;
    println!("Data directory: {}", config.data_dir()?.display());
    println!("Library directory: {}", config.library_dir()?.display());
    println!();
    println!("Setup complete. Run `recallwell` to start the server.");
    Ok(())
}

/// Show the config path and the current (redacted) configuration.
pub fn run_config(overrides: &CliOverrides, edit: bool) -> Result<()> {
    let path = Config::config_path()?;
    println!("Config path: {}", path.display());

    if edit {
        let editor = std::env::var("EDITOR").or_else(|_| {
            if cfg!(target_os = "windows") {
                Ok::<String, std::env::VarError>("notepad.exe".into())
            } else {
                Ok("vi".into())
            }
        })?;
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, toml::to_string_pretty(&Config::default())?)?;
        }
        let status = std::process::Command::new(&editor)
            .arg(&path)
            .status()
            .with_context(|| format!("failed to launch editor {editor}"))?;
        if !status.success() {
            return Err(anyhow!("editor {editor} exited with {status}"));
        }
        return Ok(());
    }

    if !path.exists() {
        println!();
        println!("(no config file yet; defaults will be used)");
        return Ok(());
    }

    let config = Config::load(overrides)?;
    let redacted = config.redacted();
    println!();
    println!("{}", toml::to_string_pretty(&redacted)?);
    Ok(())
}

/// List the `.db` files in the libraries directory.
pub fn run_libraries(overrides: &CliOverrides) -> Result<()> {
    let config = Config::load(overrides).unwrap_or_default();
    let dir = config.library_dir()?;
    if !dir.exists() {
        println!("No libraries directory yet ({}).", dir.display());
        return Ok(());
    }

    let mut entries: Vec<(PathBuf, u64)> = Vec::new();
    for e in std::fs::read_dir(&dir)? {
        let entry = e?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("db") {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            entries.push((path, size));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    if entries.is_empty() {
        println!("No libraries found in {}.", dir.display());
        return Ok(());
    }

    println!("Libraries in {}:", dir.display());
    for (path, size) in entries {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?");
        println!("  {:<24}  {:>10}", name, format_bytes(size));
    }
    Ok(())
}

fn format_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = n as f64;
    let mut idx = 0;
    while size >= 1024.0 && idx + 1 < UNITS.len() {
        size /= 1024.0;
        idx += 1;
    }
    format!("{size:.1} {}", UNITS[idx])
}
