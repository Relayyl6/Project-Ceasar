use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeConfig {
    pub node_id: String,
    pub publish_topic: String,
    pub loop_count: usize,
    pub fusion_window_ms: u64,
    pub threat_threshold: f32,
    /// Optional legacy seed.  If present it is used on first boot to populate
    /// the key file (migration path) and can be removed from the TOML after that.
    /// Omitting this field entirely is the correct long-term posture —
    /// the node generates and persists its own identity automatically.
    #[serde(default)]
    pub ed25519_seed_hex: Option<String>,
    /// Path to the persistent Ed25519 seed file.
    /// Created automatically on first boot if it does not exist.
    /// Defaults to "./node_identity.key" (relative to the working directory).
    #[serde(default = "default_key_file")]
    pub key_file: String,
    /// Project Caesar deployment domain: "agricultural" | "industrial" | "tactical" | "general"
    #[serde(default = "default_domain")]
    pub domain: String,
    #[serde(default = "default_recon_enabled")]
    pub recon_enabled: bool,
    pub location: Location,
    pub uplink: UplinkConfig,
    pub optical: OpticalSourceConfig,
    pub thermal: ThermalConfig,
    pub radar: RadarConfig,
    pub inference: InferenceConfig,
    #[serde(default)]
    pub sentinel: SentinelConfig,
    #[serde(default)]
    pub actuators: Vec<ActuatorConfig>,
}

fn default_recon_enabled() -> bool { true }
fn default_domain() -> String { "general".to_string() }
fn default_key_file() -> String { "./node_identity.key".to_string() }

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
    pub uart_port: Option<String>,
    pub baud_rate: Option<u32>,
    pub lora_frequency_mhz: Option<f32>,
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
    pub csi_port: Option<u8>,
    pub i2c_bus: Option<u8>,
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
    pub i2c_address: Option<String>,
    pub uart_port: Option<String>,
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
    pub uart_data_port: Option<String>,
    pub uart_ctrl_port: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InferenceConfig {
    pub mode: String,
    pub command_program: Option<String>,
    pub command_args: Option<Vec<String>>,
    pub model_yolo_world: Option<String>,
    pub model_gemini_er: Option<String>,
    pub model_seq2seq: Option<String>,
    pub model_pxadmm: Option<String>,
    pub vocabulary: Option<Vec<String>>,
    pub ollama_endpoint: Option<String>,
    pub ollama_model: Option<String>,
}

/// Configuration for the OpenCV-native sentinel pipeline.
/// When enabled, OpenCV becomes the always-on gatekeeper;
/// the AI inference chain only activates on sentinel signal.
#[derive(Debug, Clone, Deserialize)]
pub struct SentinelConfig {
    /// Enable the sentinel pipeline (disables the standard optical worker loop).
    #[serde(default)]
    pub enabled: bool,
    /// OpenCV VideoCapture device index (0 = first camera).
    #[serde(default)]
    pub device_id: i32,
    /// Target capture rate. OpenCV will request this from the driver.
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Minimum foreground contour area (px²) to count as motion.
    #[serde(default = "default_motion_area")]
    pub motion_area_threshold: f64,
    /// Anomaly score above this → start accumulating snapshots.
    #[serde(default = "default_anomaly_threshold")]
    pub anomaly_threshold: f32,
    /// Anomaly score above this → bypass accumulation, escalate to AI immediately.
    #[serde(default = "default_burst_threshold")]
    pub burst_threshold: f32,
    /// Number of confirmed-change snapshots required before autonomous action fires.
    #[serde(default = "default_min_snapshots")]
    pub min_snapshots: usize,
    /// Mean snapshot confidence required for autonomous actuator dispatch.
    #[serde(default = "default_autonomous_confidence")]
    pub autonomous_confidence: f32,
    /// Optional directory to persist snapshot JPEGs across reboots.
    pub snapshot_dir: Option<String>,
    /// Optional port to serve a live MJPEG stream for the dashboard.
    pub mjpeg_port: Option<u16>,
}

impl Default for SentinelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            device_id: 0,
            fps: default_fps(),
            motion_area_threshold: default_motion_area(),
            anomaly_threshold: default_anomaly_threshold(),
            burst_threshold: default_burst_threshold(),
            min_snapshots: default_min_snapshots(),
            autonomous_confidence: default_autonomous_confidence(),
            snapshot_dir: None,
            mjpeg_port: None,
        }
    }
}

fn default_fps() -> u32 { 24 }
fn default_motion_area() -> f64 { 500.0 }
fn default_anomaly_threshold() -> f32 { 0.30 }
fn default_burst_threshold() -> f32 { 0.70 }
fn default_min_snapshots() -> usize { 8 }
fn default_autonomous_confidence() -> f32 { 0.87 }

/// A pluggable effector — anything that can take autonomous action
/// in response to a Project Caesar sentinel/AI detection event.
#[derive(Debug, Clone, Deserialize)]
pub struct ActuatorConfig {
    pub id: String,
    /// "log" | "gpio" | "serial" | "mqtt" | "webhook"
    pub actuator_type: String,
    /// Actions this actuator handles, e.g. ["increase_flow", "alert"]
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Linux GPIO pin number (gpio type only).
    pub gpio_pin: Option<u32>,
    /// Serial port path, e.g. "/dev/ttyUSB0" (serial type only).
    pub serial_port: Option<String>,
    /// Serial baud rate (serial type only).
    pub baud_rate: Option<u32>,
    /// MQTT topic (mqtt type only).
    pub mqtt_topic: Option<String>,
    /// MQTT broker address in "host:port" format (mqtt type only). Defaults to "localhost:1883".
    pub mqtt_broker: Option<String>,
    /// Full HTTP URL to POST to (webhook type only), e.g. "http://192.168.1.50:8080/alert".
    pub webhook_url: Option<String>,
    /// Optional Bearer token sent in the Authorization header (webhook type only).
    pub webhook_bearer_token: Option<String>,
}
