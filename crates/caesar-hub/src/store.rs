use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};
use uriel_caesar_core::{
    io::{to_json_line, unix_time_ms},
    protocol::{SignedEnvelope, StoredEnvelopeRecord},
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

        let journal_writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)
            .await?;
        let high_interest_writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&high_interest_path)
            .await?;

        let latest_tracks = load_latest_snapshot(&latest_path).await.unwrap_or_default();

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

        {
            let payload = to_json_line(&record)?;
            let mut writer = self.journal_writer.lock().await;
            writer.write_all(payload.as_bytes()).await?;
            writer.flush().await?;
        }

        {
            let payload = {
                let mut latest = self.latest_tracks.lock().await;
                latest.insert(record.envelope.body.track_id.clone(), record.clone());
                serde_json::to_vec_pretty(&*latest)?
            };
            tokio::fs::write(&self.latest_path, payload).await?;
        }

        if record.envelope.body.threat_level == "high-interest" {
            let payload = to_json_line(&record)?;
            let mut writer = self.high_interest_writer.lock().await;
            writer.write_all(payload.as_bytes()).await?;
            writer.flush().await?;
        }

        Ok(())
    }

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

async fn load_latest_snapshot(path: &PathBuf) -> Result<HashMap<String, StoredEnvelopeRecord>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str(&raw)?)
}
