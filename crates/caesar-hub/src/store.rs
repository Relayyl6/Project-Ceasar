use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};
use uriel_caesar_core::{
    io::{to_json_line, unix_time_ms},
    protocol::{FusedTrack, SignedEnvelope, StoredEnvelopeRecord, THREAT_LEVEL_HIGH},
};
use zeromq::{Socket, SocketSend, PubSocket};

use crate::config::HubConfig;

// ── Cross-node track correlation ──────────────────────────────────────────────

/// A globally correlated track, synthesised from observations made by multiple
/// edge nodes that report the same real-world target within a 500 m / 30 s
/// spatio-temporal window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CorrelatedTrack {
    /// Canonical global track ID (e.g. "global-00001")
    pub global_track_id: String,
    /// Node IDs contributing to this track
    pub contributing_nodes: Vec<String>,
    /// Local track IDs per node (node_id → local track_id)
    pub local_track_ids: std::collections::HashMap<String, String>,
    /// Merged confidence (max of all contributors)
    pub merged_confidence: f32,
    /// Best-estimate geo position — running average of contributors that
    /// provided non-zero GPS coordinates.
    pub geo_latitude: f64,
    pub geo_longitude: f64,
    /// Last update timestamp (unix ms)
    pub last_update_ms: u64,
    /// Threat level — highest severity among contributing envelopes.
    /// "high-interest" > "monitor".
    pub threat_level: String,
    /// All contributing FusedTrack envelopes, in arrival order.
    pub envelopes: Vec<FusedTrack>,
}

// ── Haversine distance helper ─────────────────────────────────────────────────

/// Returns the great-circle distance in metres between two WGS-84 coordinates.
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    r * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

/// Returns `true` when the coordinate pair carries meaningful GPS data.
/// Nodes without a GPS fix report (0.0, 0.0) which is in the Gulf of Guinea
/// and should not be used for spatial correlation.
#[inline]
fn has_gps(lat: f64, lon: f64) -> bool {
    lat.abs() > 1e-6 || lon.abs() > 1e-6
}

// ── Threat-level ordering ─────────────────────────────────────────────────────

/// Returns the higher-severity threat level of the two strings.
/// "high-interest" > "monitor" > anything else.
fn higher_threat<'a>(a: &'a str, b: &'a str) -> &'a str {
    match (a, b) {
        (THREAT_LEVEL_HIGH, _) | (_, THREAT_LEVEL_HIGH) => THREAT_LEVEL_HIGH,
        _ => a,
    }
}

// ── Inner state shared across clone handles ───────────────────────────────────

struct HubStoreInner {
    /// In-memory map: local track_id → latest stored record.
    latest_tracks: HashMap<String, StoredEnvelopeRecord>,
    /// In-memory map: global_track_id → CorrelatedTrack.
    correlated_tracks: HashMap<String, CorrelatedTrack>,
    /// Monotonic counter used to mint new global track IDs.
    next_global_id: u64,
}

// ── Public HubStore handle ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct HubStore {
    latest_path: PathBuf,
    correlated_tracks_path: Option<PathBuf>,
    high_interest_path: PathBuf,
    journal_writer: Arc<Mutex<tokio::fs::File>>,
    high_interest_writer: Arc<Mutex<tokio::fs::File>>,
    pub_socket: Arc<Mutex<PubSocket>>,
    /// All mutable state lives behind a single Mutex so that cloned handles
    /// share the same correlation table.  The lock is *only* held for in-memory
    /// mutations — never across any blocking or async I/O.
    inner: Arc<Mutex<HubStoreInner>>,
}

impl HubStore {
    pub async fn from_config(config: &HubConfig) -> Result<Self> {
        let journal_path = PathBuf::from(&config.storage.journal_path);
        let latest_path = PathBuf::from(&config.storage.latest_path);
        let high_interest_path = PathBuf::from(&config.storage.high_interest_path);

        let correlated_tracks_path: Option<PathBuf> =
            if config.storage.correlated_tracks_path.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(&config.storage.correlated_tracks_path))
            };

        ensure_parent_dir(&journal_path).await?;
        ensure_parent_dir(&latest_path).await?;
        ensure_parent_dir(&high_interest_path).await?;
        if let Some(ref p) = correlated_tracks_path {
            ensure_parent_dir(p).await?;
        }

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

        // Load any existing correlated-tracks snapshot so correlation survives
        // hub restarts.
        let correlated_tracks = if let Some(ref p) = correlated_tracks_path {
            load_correlated_snapshot(p).await?
        } else {
            HashMap::new()
        };

        // Seed next_global_id from the highest numeric suffix already on disk so
        // we never re-issue a global track ID after a restart.
        let next_global_id = correlated_tracks
            .keys()
            .filter_map(|k| k.strip_prefix("global-")?.parse::<u64>().ok())
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);

        let mut pub_socket = PubSocket::new();
        pub_socket.bind("tcp://127.0.0.1:5555").await.with_context(|| "failed to bind ZeroMQ pub socket")?;

        Ok(Self {
            latest_path,
            correlated_tracks_path,
            high_interest_path,
            journal_writer: Arc::new(Mutex::new(journal_writer)),
            high_interest_writer: Arc::new(Mutex::new(high_interest_writer)),
            pub_socket: Arc::new(Mutex::new(pub_socket)),
            inner: Arc::new(Mutex::new(HubStoreInner {
                latest_tracks,
                correlated_tracks,
                next_global_id,
            })),
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

        // ── Latest-tracks snapshot ────────────────────────────────────────────
        // The Mutex is only held for the HashMap mutation; the tokio::fs::write
        // runs outside the lock so we do not block other tasks for the duration
        // of a syscall.
        let latest_payload = {
            let mut inner = self.inner.lock().await;
            inner
                .latest_tracks
                .insert(record.envelope.body.track_id.clone(), record.clone());
            serde_json::to_vec_pretty(&inner.latest_tracks)?
        };
        tokio::fs::write(&self.latest_path, latest_payload).await?;

        // ── Cross-node correlation ────────────────────────────────────────────
        // correlate_track() acquires + releases the Mutex internally for the
        // pure in-memory mutation, then returns the serialised payload so we can
        // write it to disk without holding the lock.
        let (corr_payload, updated_track) = self.correlate_track_and_serialize(&record.envelope).await?;
        if let Some(ref corr_path) = self.correlated_tracks_path {
            tokio::fs::write(corr_path, &corr_payload).await?;
        }

        // ── Real-Time Event Bus (ZeroMQ) ──────────────────────────────────────
        // Publish the updated track immediately to subscribers (Orchestrator Tracker)
        // for sub-50ms latency.
        {
            let payload_str = serde_json::to_string(&updated_track)?;
            let mut sock = self.pub_socket.lock().await;
            if let Err(e) = sock.send(zeromq::ZmqMessage::from(payload_str)).await {
                eprintln!("[caesar.hub] failed to publish to zmq: {e}");
            }
        }

        // ── High-interest journal ─────────────────────────────────────────────
        // Use the canonical constant so this comparison stays in sync with the
        // threat_level values defined in `protocol.rs`.
        if record.envelope.body.threat_level == THREAT_LEVEL_HIGH {
            let mut writer = self.high_interest_writer.lock().await;
            writer.write_all(json_line.as_bytes()).await?;
            writer.flush().await?;
        }

        Ok(())
    }

    /// Correlates `envelope` against existing `CorrelatedTrack` entries:
    ///
    /// * If a matching track exists (≤ 500 m **and** ≤ 30 s), merge into it.
    /// * Otherwise create a new `CorrelatedTrack` with a fresh global ID.
    ///
    /// The lock is only held for the in-memory mutation. Returns the serialised
    /// JSON of the full correlated-tracks map so the caller can write to disk
    /// without holding the lock.
    async fn correlate_track_and_serialize(
        &self,
        envelope: &SignedEnvelope,
    ) -> Result<(Vec<u8>, CorrelatedTrack)> {
        let track = &envelope.body;
        let incoming_has_gps = has_gps(track.geo_latitude, track.geo_longitude);

        let mut inner = self.inner.lock().await;

        // Find a matching global track (within 500 m and 30 s).
        let match_id: Option<String> = if incoming_has_gps {
            inner
                .correlated_tracks
                .iter()
                .find_map(|(gid, ct)| {
                    let dt_ms = track
                        .timestamp_ms
                        .abs_diff(ct.last_update_ms);
                    if dt_ms > 30_000 {
                        return None;
                    }
                    if !has_gps(ct.geo_latitude, ct.geo_longitude) {
                        return None;
                    }
                    let dist = haversine_m(
                        ct.geo_latitude,
                        ct.geo_longitude,
                        track.geo_latitude,
                        track.geo_longitude,
                    );
                    if dist <= 500.0 {
                        Some(gid.clone())
                    } else {
                        None
                    }
                })
        } else {
            // No GPS data — cannot spatially correlate; create a new entry.
            None
        };

        let updated_track;
        if let Some(ref gid) = match_id {
            // Merge into existing CorrelatedTrack.
            let ct = inner.correlated_tracks.get_mut(gid).unwrap();
            let n = ct.contributing_nodes.len() as f64;

            // Running average of geo position weighted by contributor count.
            if incoming_has_gps && has_gps(ct.geo_latitude, ct.geo_longitude) {
                ct.geo_latitude = (ct.geo_latitude * n + track.geo_latitude) / (n + 1.0);
                ct.geo_longitude = (ct.geo_longitude * n + track.geo_longitude) / (n + 1.0);
            } else if incoming_has_gps {
                ct.geo_latitude = track.geo_latitude;
                ct.geo_longitude = track.geo_longitude;
            }

            // Merge confidence (keep max).
            if track.confidence > ct.merged_confidence {
                ct.merged_confidence = track.confidence;
            }

            // Merge threat level (escalate, never de-escalate).
            let elevated = higher_threat(&ct.threat_level, &track.threat_level).to_owned();
            ct.threat_level = elevated;

            // Register this node's contribution.
            if !ct.contributing_nodes.contains(&envelope.node_id) {
                ct.contributing_nodes.push(envelope.node_id.clone());
            }
            ct.local_track_ids
                .insert(envelope.node_id.clone(), track.track_id.clone());

            ct.last_update_ms = track.timestamp_ms;
            ct.envelopes.push(track.clone());
            updated_track = ct.clone();
        } else {
            // Create a new CorrelatedTrack.
            let gid = format!("global-{:05}", inner.next_global_id);
            inner.next_global_id += 1;

            let mut local_track_ids = HashMap::new();
            local_track_ids.insert(envelope.node_id.clone(), track.track_id.clone());

            let ct = CorrelatedTrack {
                global_track_id: gid.clone(),
                contributing_nodes: vec![envelope.node_id.clone()],
                local_track_ids,
                merged_confidence: track.confidence,
                geo_latitude: track.geo_latitude,
                geo_longitude: track.geo_longitude,
                last_update_ms: track.timestamp_ms,
                threat_level: track.threat_level.clone(),
                envelopes: vec![track.clone()],
            };
            inner.correlated_tracks.insert(gid, ct.clone());
            updated_track = ct;
        }

        // Serialise while still holding the lock (avoids a second lock acquisition).
        // The resulting Vec<u8> is returned to the caller who writes it to disk
        // after the lock is dropped.
        let payload = serde_json::to_vec_pretty(&inner.correlated_tracks)?;
        Ok((payload, updated_track))
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

/// Load the correlated-tracks snapshot from `path`.
///
/// Returns an empty [`HashMap`] when the file does not yet exist (first-run).
/// Returns an error on parse failure to avoid silent data loss.
async fn load_correlated_snapshot(path: &PathBuf) -> Result<HashMap<String, CorrelatedTrack>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read correlated snapshot {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse correlated snapshot {}", path.display()))
}
