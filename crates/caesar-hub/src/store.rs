use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};
use uriel_caesar_core::{
    io::{to_json_line, unix_time_ms},
    protocol::{SignedEnvelope, StoredEnvelopeRecord, THREAT_LEVEL_HIGH},
};

use crate::config::HubConfig;

#[derive(Clone)]
pub struct HubStore {
    latest_path: PathBuf,
    high_interest_path: PathBuf,
    journal_writer: Arc<Mutex<tokio::fs::File>>,
    high_interest_writer: Arc<Mutex<tokio::fs::File>>,
    latest_tracks: Arc<Mutex<HashMap<String, StoredEnvelopeRecord>>>,
}

impl HubStore {
    pub async fn from_config(config: &HubConfig) -> Result<Self> {
        let journal_path = PathBuf::from(&config.storage.journal_path);
        let latest_path = PathBuf::from(&config.storage.latest_path);
        let high_interest_path = PathBuf::from(&config.storage.high_interest_path);

        ensure_parent_dir(&journal_path).await?;
        ensure_parent_dir(&latest_path).await?;
        ensure_parent_dir(&high_interest_path).await?;

        // KNOWN LIMITATION: The journal JSONL file (`journal_path`) is opened in append
        // mode and will grow without bound for the lifetime of a running hub process.
        // On a long-running deployment this can consume significant disk space.
        // A future improvement should implement log rotation (e.g. cap at N MiB, roll to
        // a timestamped archive, and open a fresh file) or delegate to an external
        // log-rotation daemon (logrotate / systemd-journald).
        let journal_writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)
            .await
            .with_context(|| format!("failed to open journal file {}", journal_path.display()))?;
        let high_interest_writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&high_interest_path)
            .await
            .with_context(|| {
                format!(
                    "failed to open high-interest file {}",
                    high_interest_path.display()
                )
            })?;

        // Load the latest-tracks snapshot if it exists.  If the snapshot is absent we
        // start with an empty map (normal first-run behaviour).  If the file exists but
        // cannot be parsed we return an error so the operator is aware of corruption
        // rather than silently discarding existing data.
        let latest_tracks = load_latest_snapshot(&latest_path).await?;

        Ok(Self {
            latest_path,
            high_interest_path,
            journal_writer: Arc::new(Mutex::new(journal_writer)),
            high_interest_writer: Arc::new(Mutex::new(high_interest_writer)),
            latest_tracks: Arc::new(Mutex::new(latest_tracks)),
        })
    }

    pub async fn persist(&self, envelope: SignedEnvelope) -> Result<()> {
        let record = StoredEnvelopeRecord {
            received_at_ms: unix_time_ms(),
            envelope,
        };

        // Serialise once and reuse for both journal and high-interest writes.
        let json_line = to_json_line(&record)?;

        // Write to the journal; lock is released at the end of this block.
        {
            let mut writer = self.journal_writer.lock().await;
            writer.write_all(json_line.as_bytes()).await?;
            writer.flush().await?;
        }

        // Update the in-memory latest-tracks map and atomically persist it.
        // The Mutex is only held for the HashMap mutation; the tokio::fs::write
        // runs outside the lock so we do not block other tasks for the duration
        // of a syscall.
        let payload = {
            let mut latest = self.latest_tracks.lock().await;
            latest.insert(record.envelope.body.track_id.clone(), record.clone());
            serde_json::to_vec_pretty(&*latest)?
        };
        tokio::fs::write(&self.latest_path, payload).await?;

        // Use the canonical constant so this comparison stays in sync with the
        // threat_level values defined in `protocol.rs`.
        if record.envelope.body.threat_level == THREAT_LEVEL_HIGH {
            let mut writer = self.high_interest_writer.lock().await;
            writer.write_all(json_line.as_bytes()).await?;
            writer.flush().await?;
        }

        Ok(())
    }

    /// Returns the path to the high-interest JSONL file.
    pub fn high_interest_path(&self) -> &PathBuf {
        &self.high_interest_path
    }
}

pub async fn print_latest_snapshot(config: &HubConfig) -> Result<()> {
    let path = PathBuf::from(&config.storage.latest_path);
    let snapshot = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("failed to read latest snapshot {}", path.display()))?;
    println!("{snapshot}");
    Ok(())
}

async fn ensure_parent_dir(path: &PathBuf) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    Ok(())
}

/// Load the latest-tracks snapshot from `path`.
///
/// Returns an empty [`HashMap`] when the file does not yet exist (first-run
/// behaviour).  Returns an error when the file exists but cannot be parsed,
/// so that silent data-loss from a corrupt snapshot is avoided.
async fn load_latest_snapshot(path: &PathBuf) -> Result<HashMap<String, StoredEnvelopeRecord>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read latest snapshot {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse latest snapshot {}", path.display()))
}
