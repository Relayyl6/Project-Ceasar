//! zenoh DDS bridge for Project Caesar.
//! Publishes FusedTrack events to DDS-compatible zenoh topics.
//! Compatible with ROS 2 via `zenoh-ros2-bridge` for full DDS interoperability.
//!
//! Topics published:
//!   caesar/tracks/{node_id}/{track_id}  — FusedTrack JSON
//!   caesar/alerts/{node_id}            — high-interest FusedTrack JSON
//!   caesar/telemetry/{node_id}         — node status JSON
//!
//! Topics subscribed:
//!   caesar/commands/{node_id}          — JSON commands from hub/ROS2

use anyhow::Result;
use uriel_caesar_core::protocol::FusedTrack;

#[cfg(feature = "dds")]
pub mod bridge {
    use super::*;
    use crate::config::DdsConfig;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    pub struct DdsBridge {
        session: Arc<zenoh::Session>,
        node_id: String,
        base_key: String,
    }

    impl DdsBridge {
        /// Open a zenoh session and return a bridge instance.
        pub async fn new(node_id: &str, config: &DdsConfig) -> Result<Self> {
            let mut zconfig = zenoh::Config::default();
            if let Some(ref ep) = config.router_endpoint {
                // Connect to a specific zenoh router (e.g. the hub or a ROS 2 bridge)
                let endpoints = serde_json::json!([ep]);
                zconfig.insert_json5("connect/endpoints", &endpoints.to_string())
                    .map_err(|e| anyhow::anyhow!("zenoh config error: {}", e))?;
            }
            let session = zenoh::open(zconfig).await
                .map_err(|e| anyhow::anyhow!("zenoh session failed to open: {}", e))?;
            eprintln!("[caesar.dds] zenoh session opened (node_id={})", node_id);
            Ok(Self {
                session: Arc::new(session),
                node_id: node_id.to_string(),
                base_key: config.base_key.clone(),
            })
        }

        /// Publish a FusedTrack to `{base_key}/tracks/{node_id}/{track_id}`.
        pub async fn publish_track(&self, track: &FusedTrack) -> Result<()> {
            let key = format!("{}/tracks/{}/{}",
                self.base_key, self.node_id, track.track_id);
            let json = serde_json::to_string(track)
                .map_err(|e| anyhow::anyhow!("DDS serialise error: {}", e))?;
            self.session.put(&key, json).await
                .map_err(|e| anyhow::anyhow!("zenoh put failed: {}", e))?;
            Ok(())
        }

        /// Publish a high-interest alert to `{base_key}/alerts/{node_id}`.
        pub async fn publish_alert(&self, track: &FusedTrack) -> Result<()> {
            let key = format!("{}/alerts/{}", self.base_key, self.node_id);
            let json = serde_json::to_string(track)
                .map_err(|e| anyhow::anyhow!("DDS alert serialise error: {}", e))?;
            self.session.put(&key, json).await
                .map_err(|e| anyhow::anyhow!("zenoh alert put failed: {}", e))?;
            Ok(())
        }

        /// Subscribe to commands on `{base_key}/commands/{node_id}`.
        /// Spawns a background task; returns an mpsc receiver of raw JSON command strings.
        pub async fn subscribe_commands(&self) -> Result<mpsc::Receiver<String>> {
            let key = format!("{}/commands/{}", self.base_key, self.node_id);
            let (tx, rx) = mpsc::channel::<String>(64);
            let subscriber = self.session.declare_subscriber(&key).await
                .map_err(|e| anyhow::anyhow!("zenoh subscriber failed: {}", e))?;
            tokio::spawn(async move {
                loop {
                    match subscriber.recv_async().await {
                        Ok(sample) => {
                            let payload = sample.payload();
                            // zenoh 1.0: payload is ZBytes, convert to str
                            match payload.try_to_string() {
                                Ok(s) => {
                                    let _ = tx.send(s.into_owned()).await;
                                }
                                Err(e) => {
                                    eprintln!("[caesar.dds] Command decode error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[caesar.dds] Subscriber recv error: {} — stopping", e);
                            break;
                        }
                    }
                }
            });
            eprintln!("[caesar.dds] Subscribed to commands on key '{}'", key);
            Ok(rx)
        }
    }
}

#[cfg(not(feature = "dds"))]
pub mod bridge {
    use super::*;
    use crate::config::DdsConfig;
    use tokio::sync::mpsc;

    /// Stub bridge when compiled without the `dds` feature.
    pub struct DdsBridge;
    impl DdsBridge {
        pub async fn new(_node_id: &str, _config: &DdsConfig) -> anyhow::Result<Self> {
            eprintln!("[caesar.dds] DDS bridge not compiled in (build with --features dds)");
            Ok(Self)
        }
        pub async fn publish_track(&self, _track: &FusedTrack) -> anyhow::Result<()> { Ok(()) }
        pub async fn publish_alert(&self, _track: &FusedTrack) -> anyhow::Result<()> { Ok(()) }
        pub async fn subscribe_commands(&self) -> anyhow::Result<mpsc::Receiver<String>> {
            let (_tx, rx) = mpsc::channel(1);
            Ok(rx)
        }
    }
}
