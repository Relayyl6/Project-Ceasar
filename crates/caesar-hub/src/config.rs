use anyhow::{bail, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct HubConfig {
    /// TCP address the hub will listen on, e.g. `"0.0.0.0:9000"`.
    pub listen_addr: String,
    pub storage: StorageConfig,
    /// When present and non-empty, only envelopes signed by one of these
    /// hex-encoded Ed25519 public keys are accepted (allowlist mode).
    /// When absent or empty, all structurally-valid envelopes are accepted.
    pub trusted_public_keys: Option<Vec<String>>,
}

impl HubConfig {
    /// Validates the configuration for obvious mis-configurations.
    ///
    /// Call this immediately after deserialising, before starting the server.
    pub fn validate(&self) -> Result<()> {
        if self.listen_addr.trim().is_empty() {
            bail!("`listen_addr` must not be empty (e.g. \"0.0.0.0:9000\")");
        }
        self.storage.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// Path to the append-only JSONL journal that records every accepted envelope.
    pub journal_path: String,
    /// Path to the JSON file that stores the latest snapshot for each track_id.
    pub latest_path: String,
    /// Path to the append-only JSONL file that records high-interest envelopes only.
    pub high_interest_path: String,
}

impl StorageConfig {
    fn validate(&self) -> Result<()> {
        if self.journal_path.trim().is_empty() {
            bail!("`storage.journal_path` must not be empty");
        }
        if self.latest_path.trim().is_empty() {
            bail!("`storage.latest_path` must not be empty");
        }
        if self.high_interest_path.trim().is_empty() {
            bail!("`storage.high_interest_path` must not be empty");
        }
        Ok(())
    }
}
