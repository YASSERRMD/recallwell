//! Background ingest queue.
//!
//! A small set of worker tasks pulls jobs off an mpsc channel, parses the
//! file, hands the bytes to pagebridge, and waits for summaries to finish.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use pagebridge::IngestParams;
use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use ulid::Ulid;

use crate::config::Config;
use crate::ingest::parse_bytes;
use crate::library::LibraryRegistry;
use crate::source::{SourceEntry, SourceMap};

const CHANNEL_BUFFER: usize = 64;
const BROADCAST_BUFFER: usize = 32;

/// One ingest task: file landed on disk, waiting to be parsed and shipped.
#[derive(Debug, Clone, Serialize)]
pub struct IngestJob {
    pub id: String,
    pub library: String,
    pub original_filename: String,
    pub file_path: PathBuf,
    pub state: IngestState,
}

/// Mutable progress state for a job.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngestState {
    Queued,
    Parsing { progress: f32 },
    Ingesting { progress: f32 },
    Summarizing { progress: f32 },
    Done { doc_id: String, title: String },
    Failed { error: String },
}

#[derive(Clone)]
struct Submission {
    id: Ulid,
    library: String,
    file_path: PathBuf,
    original_filename: String,
}

struct Slot {
    state: IngestState,
    broadcaster: broadcast::Sender<IngestState>,
    library: String,
    original_filename: String,
    file_path: PathBuf,
}

/// Tracks active and completed jobs.
pub struct IngestQueue {
    tx: mpsc::Sender<Submission>,
    slots: Arc<RwLock<BTreeMap<String, Slot>>>,
}

impl IngestQueue {
    /// Spawn `max_concurrent` worker tasks and return the handle.
    pub fn start(
        libraries: Arc<LibraryRegistry>,
        config: Arc<Config>,
        max_concurrent: usize,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<Submission>(CHANNEL_BUFFER);
        let slots: Arc<RwLock<BTreeMap<String, Slot>>> = Arc::new(RwLock::new(BTreeMap::new()));

        let queue = Arc::new(Self {
            tx,
            slots: slots.clone(),
        });

        let rx = Arc::new(tokio::sync::Mutex::new(rx));
        for _ in 0..max_concurrent.max(1) {
            let rx = rx.clone();
            let libraries = libraries.clone();
            let slots = slots.clone();
            let config = config.clone();
            tokio::spawn(async move {
                worker(rx, libraries, config, slots).await;
            });
        }

        queue
    }

    /// Enqueue a job that has already been written to disk.
    pub async fn submit(
        &self,
        library: &str,
        file_path: PathBuf,
        original_filename: &str,
    ) -> Result<Ulid> {
        let id = Ulid::new();
        let submission = Submission {
            id,
            library: library.to_string(),
            file_path: file_path.clone(),
            original_filename: original_filename.to_string(),
        };

        let (broadcaster, _) = broadcast::channel(BROADCAST_BUFFER);
        let slot = Slot {
            state: IngestState::Queued,
            broadcaster,
            library: library.to_string(),
            original_filename: original_filename.to_string(),
            file_path,
        };
        self.slots.write().insert(id.to_string(), slot);

        self.tx
            .send(submission)
            .await
            .map_err(|e| anyhow!("queue closed: {e}"))?;
        Ok(id)
    }

    /// Subscribe to live updates for a job.
    pub fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<IngestState>> {
        let slots = self.slots.read();
        slots.get(id).map(|s| s.broadcaster.subscribe())
    }

    /// Snapshot the current state.
    pub fn status(&self, id: &str) -> Option<IngestJob> {
        let slots = self.slots.read();
        slots.get(id).map(|s| IngestJob {
            id: id.to_string(),
            library: s.library.clone(),
            original_filename: s.original_filename.clone(),
            file_path: s.file_path.clone(),
            state: s.state.clone(),
        })
    }

    /// Snapshot all active or recent jobs.
    pub fn list(&self) -> Vec<IngestJob> {
        let slots = self.slots.read();
        slots
            .iter()
            .map(|(id, s)| IngestJob {
                id: id.clone(),
                library: s.library.clone(),
                original_filename: s.original_filename.clone(),
                file_path: s.file_path.clone(),
                state: s.state.clone(),
            })
            .collect()
    }
}

async fn worker(
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Submission>>>,
    libraries: Arc<LibraryRegistry>,
    config: Arc<Config>,
    slots: Arc<RwLock<BTreeMap<String, Slot>>>,
) {
    loop {
        let submission = {
            let mut guard = rx.lock().await;
            match guard.recv().await {
                Some(s) => s,
                None => return,
            }
        };
        let id = submission.id.to_string();
        info!(job = %id, "ingest job picked up");
        let outcome = process_job(&submission, &libraries, &config, &slots).await;
        match outcome {
            Ok((doc_id, title)) => set_state(&slots, &id, IngestState::Done { doc_id, title }),
            Err(e) => {
                warn!(job = %id, error = %e, "ingest job failed");
                set_state(
                    &slots,
                    &id,
                    IngestState::Failed {
                        error: e.to_string(),
                    },
                );
            }
        }
    }
}

async fn process_job(
    submission: &Submission,
    libraries: &Arc<LibraryRegistry>,
    config: &Arc<Config>,
    slots: &Arc<RwLock<BTreeMap<String, Slot>>>,
) -> Result<(String, String)> {
    let id = submission.id.to_string();

    // 1. Parse.
    set_state(slots, &id, IngestState::Parsing { progress: 0.0 });
    let file_path = submission.file_path.clone();
    let original_filename = submission.original_filename.clone();
    let bytes = tokio::fs::read(&file_path).await?;
    let parsed = tokio::task::spawn_blocking(move || parse_bytes(&original_filename, &bytes))
        .await
        .map_err(|e| anyhow!("parser join: {e}"))??;
    set_state(slots, &id, IngestState::Parsing { progress: 1.0 });

    // 2. Hand off to pagebridge.
    set_state(slots, &id, IngestState::Ingesting { progress: 0.0 });
    let bridge = libraries.open(&submission.library).await?;
    let params = IngestParams {
        title: parsed.title.clone(),
        source_kind: parsed.source_kind,
        raw_text: parsed.raw,
        doc_id: None,
        user_metadata: parsed.metadata,
    };
    let handle = bridge
        .ingest_document(params)
        .await
        .map_err(|e| anyhow!("pagebridge ingest: {e}"))?;
    set_state(slots, &id, IngestState::Ingesting { progress: 1.0 });

    // 3. Wait for background summaries.
    set_state(slots, &id, IngestState::Summarizing { progress: 0.0 });
    bridge
        .wait_for_summaries(&handle.doc_id)
        .await
        .map_err(|e| anyhow!("pagebridge summaries: {e}"))?;
    set_state(slots, &id, IngestState::Summarizing { progress: 1.0 });

    let doc_id = handle.doc_id.to_string();

    // 4. Persist doc_id -> source file mapping so click-to-source works.
    if let Ok(map) = SourceMap::open(config.clone(), &submission.library) {
        let entry = SourceEntry {
            file_path: submission.file_path.clone(),
            original_filename: submission.original_filename.clone(),
            ingested_at: now_secs(),
        };
        if let Err(e) = map.record(&doc_id, entry) {
            tracing::warn!(job = %submission.id, "source map persist failed: {e}");
        }
    }

    Ok((doc_id, parsed.title))
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

fn set_state(slots: &Arc<RwLock<BTreeMap<String, Slot>>>, id: &str, state: IngestState) {
    let mut guard = slots.write();
    if let Some(slot) = guard.get_mut(id) {
        slot.state = state.clone();
        // Don't fail if no subscribers.
        let _ = slot.broadcaster.send(state);
    } else {
        error!(job = %id, "set_state for unknown job");
    }
}
