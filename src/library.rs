//! Library registry: maps named libraries (`reading`, `work`, ...) to
//! pagebridge instances backed by their own SQLite database.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use dashmap::DashMap;
use pagebridge::{OpenAiCompatibleProvider, Pagebridge, SqliteAdapter};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::config::Config;

const NAME_MAX: usize = 64;

/// Per-library statistics surfaced to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryInfo {
    pub name: String,
    pub file_size_bytes: u64,
    pub document_count: u32,
    pub last_used: Option<i64>,
}

/// State persisted in `state.json` to remember the active library.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryState {
    #[serde(default)]
    pub active_library: Option<String>,
    #[serde(default)]
    pub last_used_at: Option<i64>,
}

/// Caches open pagebridge instances and creates new libraries on demand.
pub struct LibraryRegistry {
    config: Arc<Config>,
    libraries: DashMap<String, Arc<Pagebridge>>,
    state: Mutex<LibraryState>,
    state_path: PathBuf,
}

impl LibraryRegistry {
    /// Build a registry rooted at the configured data directory.
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let state_path = config.state_path()?;
        let state = load_state(&state_path).unwrap_or_default();
        Ok(Self {
            config,
            libraries: DashMap::new(),
            state: Mutex::new(state),
            state_path,
        })
    }

    /// Open (or create) the library with the given name.
    ///
    /// The returned Pagebridge is cached; calling `open` again with the same
    /// name returns the same `Arc`.
    pub async fn open(&self, name: &str) -> Result<Arc<Pagebridge>> {
        validate_name(name)?;
        if let Some(existing) = self.libraries.get(name) {
            return Ok(existing.clone());
        }
        let path = self.path_for(name)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let storage = Arc::new(
            SqliteAdapter::open(&path)
                .await
                .with_context(|| format!("opening sqlite at {}", path.display()))?,
        );
        let api_key = self
            .config
            .groq
            .api_key
            .clone()
            .ok_or_else(|| anyhow!("groq API key not configured"))?;
        let llm = Arc::new(OpenAiCompatibleProvider::groq(
            api_key,
            &self.config.groq.synthesis_model,
        ));
        let bridge = Arc::new(
            Pagebridge::new(storage, llm)
                .await
                .map_err(|e| anyhow!("pagebridge init: {e}"))?,
        );
        self.libraries.insert(name.to_string(), bridge.clone());
        Ok(bridge)
    }

    /// List all `.db` files in the library directory along with metadata.
    pub async fn list(&self) -> Result<Vec<LibraryInfo>> {
        let dir = self.config.library_dir()?;
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("db") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let meta = entry.metadata()?;
            // Document count requires opening the library; for the list view
            // we skip that cost. The UI fetches detailed stats on demand.
            out.push(LibraryInfo {
                name: name.to_string(),
                file_size_bytes: meta.len(),
                document_count: 0,
                last_used: None,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Create a fresh library; opens it immediately so the SQLite file is
    /// created on disk.
    pub async fn create(&self, name: &str) -> Result<LibraryInfo> {
        validate_name(name)?;
        let path = self.path_for(name)?;
        if path.exists() {
            return Err(anyhow!("library `{name}` already exists"));
        }
        self.open(name).await?;
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Ok(LibraryInfo {
            name: name.to_string(),
            file_size_bytes: size,
            document_count: 0,
            last_used: None,
        })
    }

    /// Delete the library file and remove it from the cache.
    pub async fn delete(&self, name: &str) -> Result<()> {
        validate_name(name)?;
        self.libraries.remove(name);
        let path = self.path_for(name)?;
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        // If the active library was deleted, clear it.
        let mut state = self.state.lock().await;
        if state.active_library.as_deref() == Some(name) {
            state.active_library = None;
            self.persist_state(&state)?;
        }
        Ok(())
    }

    /// Return the active library name, or "default" if none is set.
    pub async fn active(&self) -> String {
        let state = self.state.lock().await;
        state
            .active_library
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    /// Set the active library, creating the file if it does not yet exist.
    pub async fn set_active(&self, name: &str) -> Result<()> {
        validate_name(name)?;
        // Touch it so the file exists.
        let _ = self.open(name).await?;
        let mut state = self.state.lock().await;
        state.active_library = Some(name.to_string());
        state.last_used_at = Some(now_secs());
        self.persist_state(&state)?;
        Ok(())
    }

    fn path_for(&self, name: &str) -> Result<PathBuf> {
        Ok(self.config.library_dir()?.join(format!("{name}.db")))
    }

    fn persist_state(&self, state: &LibraryState) -> Result<()> {
        if let Some(parent) = self.state_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(state)?;
        std::fs::write(&self.state_path, serialized)?;
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > NAME_MAX {
        return Err(anyhow!(
            "library name must be 1..={NAME_MAX} characters, got {}",
            name.len()
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(anyhow!(
            "library name must be lowercase alphanumeric with `-` or `_`"
        ));
    }
    Ok(())
}

fn load_state(path: &PathBuf) -> Result<LibraryState> {
    let raw = std::fs::read_to_string(path)?;
    let state: LibraryState = serde_json::from_str(&raw)?;
    Ok(state)
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation() {
        assert!(validate_name("reading").is_ok());
        assert!(validate_name("work-2026").is_ok());
        assert!(validate_name("my_notes").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("UPPER").is_err());
        assert!(validate_name("has space").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name(&"x".repeat(65)).is_err());
    }
}
