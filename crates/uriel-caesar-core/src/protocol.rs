use serde::{Deserialize, Serialize};

/// Allowed threat_level values emitted by FusionEngine.
pub const THREAT_LEVEL_HIGH: &str = "high-interest";
pub const THREAT_LEVEL_MONITOR: &str = "monitor";

/// All recognised threat_level values, in descending severity order.
pub const THREAT_LEVEL_VALID_VALUES: &[&str] = &[THREAT_LEVEL_HIGH, THREAT_LEVEL_MONITOR];

/// Returns `true` when `level` is one of the recognised threat_level values.
///
/// Use this when accepting threat_level from an external source to guard
/// against arbitrary strings reaching downstream consumers.
pub fn is_valid_threat_level(level: &str) -> bool {
    THREAT_LEVEL_VALID_VALUES.contains(&level)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Optical,
    Thermal,
    Radar,
    Manual,
    /// OpenCV sentinel-sourced observation — promoted from visual diff to AI classification.
    Sentinel,
}

/// Rich dashboard telemetry event emitted by the sentinel pipeline.
/// Carries the full context chain: detection → portrait → AI verdict → actuator result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentinelAlert {
    pub node_id: String,
    pub timestamp_ms: u64,
    /// Project Caesar domain: "agricultural" | "industrial" | "tactical" | "general"
    pub domain: String,
    /// Number of confirmed-change snapshots accumulated before this alert.
    pub snapshot_count: usize,
    /// Mean anomaly confidence across accumulated snapshots (0.0–1.0).
    pub mean_confidence: f32,
    /// AI-derived class label (from YOLO-World / Ollama / heuristic).
    pub class_label: Option<String>,
    /// Base64-encoded 2×2 composite JPEG of the 4 most significant change regions.
    pub portrait_jpeg_b64: Option<String>,
    /// Human-readable reasoning chain for dashboard display.
    pub rationale: String,
    /// Actuator actions dispatched autonomously (empty if confidence gate not met).
    pub actuator_results: Vec<String>,
    /// Which inference stage produced the class label: "onnx" | "ollama" | "heuristic" | "none"
    pub inference_stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub track_hint: String,
    pub timestamp_ms: u64,
    pub modality: Modality,
    pub confidence: f32,
    pub class_label: String,
    pub position_m: (f32, f32),
    pub velocity_mps: Option<f32>,
    pub source_id: String,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedTrack {
    pub node_id: String,
    pub timestamp_ms: u64,
    pub track_id: String,
    pub site: String,
    pub geo_latitude: f64,
    pub geo_longitude: f64,
    /// Threat classification string.  Must be one of [`THREAT_LEVEL_VALID_VALUES`].
    pub threat_level: String,
    pub confidence: f32,
    pub position_m: (f32, f32),
    pub velocity_mps: Option<f32>,
    pub contributing_modalities: Vec<Modality>,
    pub source_ids: Vec<String>,
    pub evidence_digests: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    pub schema_version: u8,
    pub node_id: String,
    pub topic: String,
    pub body: FusedTrack,
    pub public_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEnvelopeRecord {
    pub received_at_ms: u64,
    pub envelope: SignedEnvelope,
}
