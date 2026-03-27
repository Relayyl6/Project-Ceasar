use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeConfig {
    pub node_id: String,
    pub publish_topic: String,
    pub loop_count: usize,
    pub fusion_window_ms: u64,
    pub threat_threshold: f32,
    pub ed25519_seed_hex: String,
    pub location: Location,
    pub uplink: UplinkConfig,
    pub optical: OpticalSourceConfig,
    pub thermal: ThermalConfig,
    pub radar: RadarConfig,
    pub inference: InferenceConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Location {
    pub site: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UplinkConfig {
    pub mode: String,
    pub tcp_addr: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpticalSourceConfig {
    pub enabled: bool,
    pub mode: String,
    pub profile: Option<String>,
    pub camera_id: String,
    pub width: u32,
    pub height: u32,
    pub frame_interval_ms: u64,
    pub device: Option<String>,
    pub pixel_format: Option<String>,
    pub command_program: Option<String>,
    pub command_args: Option<Vec<String>>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThermalConfig {
    pub enabled: bool,
    pub mode: String,
    pub camera_id: String,
    pub width: u32,
    pub height: u32,
    pub frame_interval_ms: u64,
    pub command_program: Option<String>,
    pub command_args: Option<Vec<String>>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RadarConfig {
    pub enabled: bool,
    pub mode: String,
    pub radar_id: String,
    pub point_count: usize,
    pub frame_interval_ms: u64,
    pub command_program: Option<String>,
    pub command_args: Option<Vec<String>>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InferenceConfig {
    pub mode: String,
    pub command_program: Option<String>,
    pub command_args: Option<Vec<String>>,
}
