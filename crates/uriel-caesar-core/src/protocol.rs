use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Optical,
    Thermal,
    Radar,
    Manual,
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
