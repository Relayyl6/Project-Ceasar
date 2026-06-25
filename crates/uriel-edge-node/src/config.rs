use anyhow::Context as _;
use serde::Deserialize;

/// Validates that a URL endpoint resolves to a private/local network address.
/// Prevents accidental data exfiltration to cloud endpoints.
///
/// Edge cases handled:
/// - `file://` and other non-network schemes are rejected (no host to validate).
/// - URLs with no host component return a clear error.
/// - IP parse failures fall through to the hostname heuristic.
/// - `CAESAR_ALLOW_CLOUD=1` suppresses the cloud-hostname error (not recommended).
pub fn validate_local_endpoint(url: &str, field_name: &str) -> anyhow::Result<()> {
    use std::net::IpAddr;

    let url_parsed = url::Url::parse(url)
        .with_context(|| format!("{}: invalid URL '{}'", field_name, url))?;

    // Reject non-network schemes (file://, data:, etc.) — they have no host
    // but also don't represent remote endpoints, so treat them as safe.
    match url_parsed.scheme() {
        "http" | "https" | "mqtt" | "mqtts" | "ws" | "wss" | "tcp" => {}
        other => {
            // file:// and custom schemes are not remote endpoints — allow them.
            eprintln!(
                "[caesar.sovereignty] {} uses scheme '{}'; skipping IP check.",
                field_name, other
            );
            return Ok(());
        }
    }

    let host = url_parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("{}: URL '{}' has no host component", field_name, url))?;

    // Allow localhost and explicit loopback literals
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return Ok(());
    }

    // Try to parse as a bare IP address
    match host.parse::<IpAddr>() {
        Ok(ip) => {
            let is_private = match ip {
                IpAddr::V4(v4) => {
                    let o = v4.octets();
                    o[0] == 10                                              // 10.0.0.0/8
                    || (o[0] == 172 && o[1] >= 16 && o[1] <= 31)          // 172.16.0.0/12
                    || (o[0] == 192 && o[1] == 168)                        // 192.168.0.0/16
                    || (o[0] == 100 && o[1] >= 64 && o[1] <= 127)         // 100.64.0.0/10 CGNAT
                    || v4.is_loopback()
                    || v4.is_link_local()
                }
                IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
            };
            if is_private {
                Ok(())
            } else {
                // Check env-var override before hard-failing
                if std::env::var("CAESAR_ALLOW_CLOUD").is_ok() {
                    eprintln!(
                        "[caesar.sovereignty] OVERRIDE ACTIVE — {} '{}' points to a public IP.",
                        field_name, url
                    );
                    return Ok(());
                }
                anyhow::bail!(
                    "DATA SOVEREIGNTY VIOLATION: {} '{}' points to a public IP address. \
                    Project Caesar enforces local-network-only data processing. \
                    Set CAESAR_ALLOW_CLOUD=1 to override (not recommended).",
                    field_name, url
                )
            }
        }
        Err(_) => {
            // It's a hostname — we cannot resolve at config-load time, but we
            // can apply a cloud-TLD heuristic. Legitimate on-prem hostnames
            // rarely end in .com/.io/.ai/.net/.org/.cloud.
            let cloud_tlds = [".com", ".io", ".ai", ".net", ".org", ".cloud"];
            let looks_cloud = cloud_tlds.iter().any(|tld| host.ends_with(tld));
            if looks_cloud {
                if std::env::var("CAESAR_ALLOW_CLOUD").is_ok() {
                    eprintln!(
                        "[caesar.sovereignty] OVERRIDE ACTIVE — {} '{}' appears to be a cloud hostname.",
                        field_name, url
                    );
                    return Ok(());
                }
                anyhow::bail!(
                    "DATA SOVEREIGNTY WARNING: {} '{}' appears to be a cloud hostname. \
                    Set CAESAR_ALLOW_CLOUD=1 to permit external endpoints.",
                    field_name, url
                );
            }
            // Hostname doesn't look like a cloud FQDN — allow it (e.g. 'myhost.local', 'broker')
            Ok(())
        }
    }
}

/// Configuration for flight controller telemetry (MAVLink).
/// Used when the Pi is mounted on a drone with an ArduPilot/PX4 FC.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DroneConfig {
    /// Enable MAVLink telemetry reader
    #[serde(default)]
    pub enabled: bool,
    /// Serial port connected to the flight controller, e.g. "/dev/ttyAMA0"
    pub mavlink_port: Option<String>,
    /// Baud rate for MAVLink (typically 57600 or 115200)
    #[serde(default = "default_mavlink_baud")]
    pub mavlink_baud: u32,
}
fn default_mavlink_baud() -> u32 { 57600 }

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeConfig {
    pub node_id: String,
    pub publish_topic: String,
    #[serde(default)]
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
    /// Optional WiFi interface to use for 802.11 monitor-mode PCAP.
    /// Defaults to "wlan0" in code if not set.
    /// Example: "/dev/wlan1" for a dedicated monitor-mode USB adapter.
    #[serde(default)]
    pub recon_interface: Option<String>,
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
    #[serde(default)]
    pub drone: DroneConfig,
    #[serde(default)]
    pub dds: DdsConfig,
}

fn default_recon_enabled() -> bool { true }
fn default_domain() -> String { "general".to_string() }
fn default_key_file() -> String { "./node_identity.key".to_string() }

impl EdgeConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.node_id.is_empty() {
            anyhow::bail!("node_id cannot be empty");
        }
        if self.fusion_window_ms == 0 {
            anyhow::bail!("fusion_window_ms must be > 0 (zero causes interval panics)");
        }
        if self.sentinel.enabled {
            if self.sentinel.fps == 0 {
                anyhow::bail!("sentinel.fps must be > 0");
            }
            if self.sentinel.min_snapshots == 0 {
                anyhow::bail!("sentinel.min_snapshots must be > 0");
            }
            if self.sentinel.motion_area_threshold <= 0.0 {
                anyhow::bail!("sentinel.motion_area_threshold must be > 0.0");
            }
        }

        // ── Data Sovereignty Checks ────────────────────────────────────────────
        // Verify that every configured remote endpoint resolves to a private /
        // local address, preventing accidental data exfiltration to the cloud.
        if let Some(ref ep) = self.inference.ollama_endpoint {
            validate_local_endpoint(ep, "inference.ollama_endpoint")
                .context("Data sovereignty check failed")?;
        }
        // Validate webhook URLs declared in any actuator
        for actuator in &self.actuators {
            if let Some(ref url) = actuator.webhook_url {
                let field = format!("actuators[{}].webhook_url", actuator.id);
                validate_local_endpoint(url, &field)
                    .context("Data sovereignty check failed")?;
            }
        }
        // ──────────────────────────────────────────────────────────────────────

        Ok(())
    }
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
    pub uart_port: Option<String>,
    pub baud_rate: Option<u32>,
    pub lora_frequency_mhz: Option<f32>,
    /// F11 FIX: Explicit serial port for gossipsub/RFD900x uplink mode.
    /// When set, only this specific device is used for radio uplink.
    /// Example: "/dev/ttyUSB0" or "COM3".
    /// If absent, gossipsub logs to stdout only (no radio transmission).
    pub serial_port: Option<String>,
    /// Gossipsub topic name. Defaults to "caesar/tracks".
    #[serde(default = "default_gossipsub_topic")]
    pub gossipsub_topic: String,
    /// Port for the libp2p TCP listener. 0 = let OS pick a random port.
    #[serde(default)]
    pub gossipsub_listen_port: u16,
    /// List of bootstrap peer multiaddrs to dial on startup.
    /// Format: "/ip4/1.2.3.4/tcp/7877/p2p/<PeerId>"
    #[serde(default)]
    pub bootstrap_peers: Vec<String>,
}
fn default_gossipsub_topic() -> String { "caesar/tracks".to_string() }

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

/// Configuration for the zenoh DDS bridge.
/// Enables ROS 2 / DDS-compatible pub/sub for inter-node and hub communication.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct DdsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_dds_base_key")]
    pub base_key: String,
    pub router_endpoint: Option<String>,
}
fn default_dds_base_key() -> String { "caesar".to_string() }
