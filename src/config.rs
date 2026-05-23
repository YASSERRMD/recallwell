//! Configuration loading and validation for recallwell.
//!
//! Precedence: CLI flags > environment variables > config file > defaults.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub groq: GroqConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub data: DataConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub ingest: IngestConfig,
    #[serde(default)]
    pub ask: AskConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroqConfig {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_synthesis_model")]
    pub synthesis_model: String,
    #[serde(default = "default_navigation_model")]
    pub navigation_model: String,
    #[serde(default = "default_groq_base_url")]
    pub base_url: String,
}

impl Default for GroqConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            synthesis_model: default_synthesis_model(),
            navigation_model: default_navigation_model(),
            base_url: default_groq_base_url(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub auto_open: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            auto_open: true,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DataConfig {
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IngestConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default)]
    pub ocr_fallback: bool,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            ocr_fallback: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AskConfig {
    #[serde(default = "default_max_navigation_steps")]
    pub max_navigation_steps: u32,
    #[serde(default = "default_beam_width")]
    pub beam_width: u32,
    #[serde(default = "default_bm25_candidate_limit")]
    pub bm25_candidate_limit: u32,
    #[serde(default = "default_max_leaves")]
    pub max_leaves: u32,
    #[serde(default = "default_synthesis_temperature")]
    pub synthesis_temperature: f32,
    #[serde(default = "default_navigation_temperature")]
    pub navigation_temperature: f32,
}

impl Default for AskConfig {
    fn default() -> Self {
        Self {
            max_navigation_steps: default_max_navigation_steps(),
            beam_width: default_beam_width(),
            bm25_candidate_limit: default_bm25_candidate_limit(),
            max_leaves: default_max_leaves(),
            synthesis_temperature: default_synthesis_temperature(),
            navigation_temperature: default_navigation_temperature(),
        }
    }
}

fn default_synthesis_model() -> String {
    "llama-3.3-70b-versatile".into()
}
fn default_navigation_model() -> String {
    "llama-3.1-8b-instant".into()
}
fn default_groq_base_url() -> String {
    "https://api.groq.com/openai/v1".into()
}
fn default_host() -> String {
    "127.0.0.1".into()
}
const fn default_port() -> u16 {
    7676
}
const fn default_true() -> bool {
    true
}
fn default_theme() -> String {
    "auto".into()
}
const fn default_max_concurrent() -> usize {
    2
}
const fn default_max_navigation_steps() -> u32 {
    4
}
const fn default_beam_width() -> u32 {
    3
}
const fn default_bm25_candidate_limit() -> u32 {
    30
}
const fn default_max_leaves() -> u32 {
    8
}
const fn default_synthesis_temperature() -> f32 {
    0.2
}
const fn default_navigation_temperature() -> f32 {
    0.0
}

/// CLI overrides supplied at runtime.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub data_dir: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub port: Option<u16>,
    pub auto_open: Option<bool>,
}

const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "recallwell";
const APPLICATION: &str = "recallwell";

impl Config {
    /// Load configuration following the precedence rules.
    ///
    /// Order (highest wins): CLI overrides, environment variables,
    /// config file, hard-coded defaults.
    pub fn load(overrides: &CliOverrides) -> Result<Self> {
        let config_path = match &overrides.config_path {
            Some(p) => p.clone(),
            None => Self::config_path()?,
        };

        let mut config = if config_path.exists() {
            let raw = std::fs::read_to_string(&config_path)
                .with_context(|| format!("reading config file {}", config_path.display()))?;
            toml::from_str::<Self>(&raw)
                .with_context(|| format!("parsing config file {}", config_path.display()))?
        } else {
            Self::default()
        };

        if let Ok(key) = std::env::var("RECALLWELL_GROQ_API_KEY") {
            if !key.trim().is_empty() {
                config.groq.api_key = Some(key);
            }
        }
        if let Ok(dir) = std::env::var("RECALLWELL_DATA_DIR") {
            if !dir.trim().is_empty() {
                config.data.dir = Some(PathBuf::from(dir));
            }
        }
        if let Ok(port_str) = std::env::var("RECALLWELL_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                config.server.port = port;
            }
        }

        if let Some(dir) = &overrides.data_dir {
            config.data.dir = Some(dir.clone());
        }
        if let Some(port) = overrides.port {
            config.server.port = port;
        }
        if let Some(open) = overrides.auto_open {
            config.server.auto_open = open;
        }

        Ok(config)
    }

    /// OS-standard config file location.
    pub fn config_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
            .ok_or_else(|| anyhow!("could not determine OS config directory"))?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// OS-standard data directory (or user override).
    pub fn data_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = &self.data.dir {
            return Ok(dir.clone());
        }
        let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
            .ok_or_else(|| anyhow!("could not determine OS data directory"))?;
        Ok(dirs.data_dir().to_path_buf())
    }

    /// Directory holding library `.db` files.
    pub fn library_dir(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("libraries"))
    }

    /// Path to the history database.
    pub fn history_db_path(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("history.db"))
    }

    /// Path to the directory that mirrors ingested source files.
    pub fn ingested_files_dir(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("ingested-files"))
    }

    /// Path to the state.json file (active library, etc.).
    pub fn state_path(&self) -> Result<PathBuf> {
        Ok(self.data_dir()?.join("state.json"))
    }

    /// Validate the loaded config and create required directories.
    pub fn validate(&self) -> Result<()> {
        match &self.groq.api_key {
            Some(k) if !k.trim().is_empty() => {}
            _ => {
                return Err(anyhow!(
                    "Groq API key not set. Run `recallwell setup` or set RECALLWELL_GROQ_API_KEY."
                ));
            }
        }

        if self.groq.synthesis_model.trim().is_empty() {
            return Err(anyhow!("groq.synthesis_model is empty"));
        }
        if self.groq.navigation_model.trim().is_empty() {
            return Err(anyhow!("groq.navigation_model is empty"));
        }

        if self.server.port < 1024 {
            return Err(anyhow!(
                "server.port {} is privileged; pick a port >= 1024",
                self.server.port
            ));
        }

        let data_dir = self.data_dir()?;
        ensure_writable_dir(&data_dir)
            .with_context(|| format!("preparing data directory {}", data_dir.display()))?;
        ensure_writable_dir(&self.library_dir()?)?;
        ensure_writable_dir(&self.ingested_files_dir()?)?;

        Ok(())
    }

    /// Save the config to the OS-standard location, creating parents.
    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating config directory {}", parent.display())
            })?;
        }
        let serialized = toml::to_string_pretty(self).context("serializing config")?;
        std::fs::write(&path, serialized)
            .with_context(|| format!("writing config file {}", path.display()))?;
        Ok(path)
    }

    /// Redact secrets for display.
    pub fn redacted(&self) -> Self {
        let mut c = self.clone();
        if c.groq.api_key.is_some() {
            c.groq.api_key = Some("***redacted***".into());
        }
        c
    }
}

fn ensure_writable_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .with_context(|| format!("creating directory {}", path.display()))?;
    }
    let probe = path.join(".recallwell-probe");
    std::fs::write(&probe, b"ok")
        .with_context(|| format!("writing to {} (is it writable?)", path.display()))?;
    std::fs::remove_file(&probe).ok();
    Ok(())
}
