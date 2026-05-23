//! Doc-id to source-file mapping. Lets the UI re-open the original document
//! at the right page after an answer cites a passage.
//!
//! Persisted as a JSON file at `<data_dir>/ingested-files/<library>/sources.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::Config;

#[derive(Debug, Default, Serialize, Deserialize)]
struct OnDisk {
    #[serde(default)]
    entries: BTreeMap<String, SourceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub file_path: PathBuf,
    pub original_filename: String,
    pub ingested_at: i64,
}

/// In-memory mirror of the per-library sources map.
pub struct SourceMap {
    config: Arc<Config>,
    library: String,
    inner: RwLock<OnDisk>,
    path: PathBuf,
}

impl SourceMap {
    pub fn open(config: Arc<Config>, library: &str) -> Result<Self> {
        let path = source_map_path(&config, library)?;
        let inner = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_str::<OnDisk>(&raw).unwrap_or_default()
        } else {
            OnDisk::default()
        };
        Ok(Self {
            config,
            library: library.to_string(),
            inner: RwLock::new(inner),
            path,
        })
    }

    pub fn record(&self, doc_id: &str, entry: SourceEntry) -> Result<()> {
        {
            let mut guard = self.inner.write();
            guard.entries.insert(doc_id.to_string(), entry);
        }
        self.persist()
    }

    pub fn get(&self, doc_id: &str) -> Option<SourceEntry> {
        self.inner.read().entries.get(doc_id).cloned()
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let guard = self.inner.read();
        let raw = serde_json::to_string_pretty(&*guard)?;
        std::fs::write(&self.path, raw)?;
        Ok(())
    }
}

fn source_map_path(config: &Config, library: &str) -> Result<PathBuf> {
    Ok(config
        .ingested_files_dir()?
        .join(library)
        .join("sources.json"))
}

/// Look up the source entry for a doc-id; opens (and possibly creates) the
/// map file on demand.
pub fn lookup(config: &Arc<Config>, library: &str, doc_id: &str) -> Option<SourceEntry> {
    match SourceMap::open(config.clone(), library) {
        Ok(map) => map.get(doc_id),
        Err(e) => {
            warn!(library, doc_id, "source map open failed: {e}");
            None
        }
    }
}

/// Sanity check: a path is safely inside the ingested-files dir.
pub fn is_within_data_dir(path: &Path, config: &Config) -> bool {
    if let (Ok(canon_path), Ok(canon_root)) = (
        std::fs::canonicalize(path),
        std::fs::canonicalize(config.ingested_files_dir().unwrap_or_default()),
    ) {
        canon_path.starts_with(canon_root)
    } else {
        false
    }
}
