use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct HubConfig {
    pub listen_addr: String,
    pub storage: StorageConfig,
    pub trusted_public_keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub journal_path: String,
    pub latest_path: String,
    pub high_interest_path: String,
}
