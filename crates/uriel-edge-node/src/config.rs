use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeConfig {
    pub node_id: String,
    pub publish_topic: String,
    pub loop_count: usize,
    pub fusion_window_ms: u64,
    pub threat_threshold: f32,
    pub ed25519_seed_hex: String,
    #[serde(default = "default_recon_enabled")]
    pub recon_enabled: bool,
    pub location: Location,
    pub uplink: UplinkConfig,
    pub optical: OpticalSourceConfig,
    pub thermal: ThermalConfig,
    pub radar: RadarConfig,
    pub inference: InferenceConfig,
}

fn default_recon_enabled() -> bool { true }

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
    pub uart_port: Option<String>, // e.g., "/dev/ttyUSB0" for RFD900x
    pub baud_rate: Option<u32>,    // e.g., 57600
    pub lora_frequency_mhz: Option<f32>, // e.g., 868.0
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
    pub csi_port: Option<u8>, // Real Hardware: MIPI CSI-2 port (e.g., 0 or 1 for Pi 5)
    pub i2c_bus: Option<u8>,  // Real Hardware: I2C bus for camera control
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
    pub i2c_address: Option<String>, // Real Hardware: e.g., "0x2A" for FLIR Boson+
    pub uart_port: Option<String>,   // Real Hardware: e.g., "/dev/ttyAMA1"
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
    pub uart_data_port: Option<String>, // Real Hardware: e.g., "/dev/ttyUSB1" for TI IWR6843 Data
    pub uart_ctrl_port: Option<String>, // Real Hardware: e.g., "/dev/ttyUSB0" for TI IWR6843 Ctrl
}

#[derive(Debug, Clone, Deserialize)]
pub struct InferenceConfig {
    pub mode: String,
    pub command_program: Option<String>,
    pub command_args: Option<Vec<String>>,
    pub model_yolo_world: Option<String>, // Path to YOLO-World ONNX
    pub model_gemini_er: Option<String>,  // Path to Gemini Robotics-ER ONNX
    pub model_seq2seq: Option<String>,    // Path to Seq2Seq LSTM ONNX
    pub model_pxadmm: Option<String>,     // Path to pxADMM anomaly ONNX
    pub vocabulary: Option<Vec<String>>,  // Open-vocabulary list for YOLO-World (can exceed 80, e.g. 2000+)
    pub ollama_endpoint: Option<String>,  // Ollama endpoint, e.g., "http://localhost:11434"
    pub ollama_model: Option<String>,     // e.g. "llava" or "llama3.2-vision"
}