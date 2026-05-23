//! Configuration loading and validation for recallwell.
//!
//! Precedence: CLI flags > environment variables > config file > defaults.

use std::path::PathBuf;

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
