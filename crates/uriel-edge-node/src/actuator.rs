//! Project Caesar — Pluggable Actuator Bus
//!
//! Any physical or logical effector registers here by implementing the
//! `Actuator` trait and declaring its `capabilities`. The `ActuatorBus`
//! dispatches `ActuatorCommand`s to every actuator that handles the
//! requested action, providing a full audit trail through the always-present
//! `LogActuator`.
//!
//! Domain-aware action mapping (`infer_action_from_class`) translates an
//! AI-derived class label into the correct action for the deployment context:
//! agricultural, industrial, tactical, or general.

use anyhow::Result;

use crate::config::ActuatorConfig;

// ---------------------------------------------------------------------------
// Command and result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActuatorCommand {
    /// Action identifier, e.g. "increase_flow", "alert", "lock_perimeter".
    pub action: String,
    /// Target zone or device, e.g. "zone_3", "perimeter_north", "auto".
    pub target: String,
    /// Normalised intensity 0.0–1.0 (meaning depends on actuator type).
    pub intensity: f32,
    /// Human-readable explanation for dashboard display and audit logs.
    pub rationale: String,
    /// Composite confidence driving this command (anomaly × AI confidence).
    pub confidence: f32,
    /// Project Caesar domain context for log enrichment.
    pub domain: String,
}

#[derive(Debug, Clone)]
pub struct ActuatorResult {
    pub actuator_id: String,
    pub success: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Actuator trait
// ---------------------------------------------------------------------------

pub trait Actuator: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> &[String];
    fn execute(&self, cmd: &ActuatorCommand) -> Result<ActuatorResult>;
}

// ---------------------------------------------------------------------------
// LogActuator — always registered; feeds rich Caesar-domain telemetry
// ---------------------------------------------------------------------------

pub struct LogActuator {
    id: String,
    caps: Vec<String>,
}

impl LogActuator {
    pub fn new(id: impl Into<String>, caps: Vec<String>) -> Self {
        Self { id: id.into(), caps }
    }
}

impl Actuator for LogActuator {
    fn id(&self) -> &str { &self.id }
    fn capabilities(&self) -> &[String] { &self.caps }

    fn execute(&self, cmd: &ActuatorCommand) -> Result<ActuatorResult> {
        // Emit domain-prefixed log line readable by the dashboard parser
        println!(
            "[caesar.{domain}][actuator.{id}] ACTION={action} TARGET={target} \
             INTENSITY={intensity:.2} CONFIDENCE={confidence:.2}\n  ↳ {rationale}",
            domain    = cmd.domain,
            id        = self.id,
            action    = cmd.action,
            target    = cmd.target,
            intensity = cmd.intensity,
            confidence= cmd.confidence,
            rationale = cmd.rationale,
        );
        Ok(ActuatorResult {
            actuator_id: self.id.clone(),
            success: true,
            message: format!("[{}] {} → {}", cmd.domain, cmd.action, cmd.target),
        })
    }
}

// ---------------------------------------------------------------------------
// SerialActuator — MCU-controlled effectors (irrigation valves, relays, etc.)
// Protocol: "ACTION:TARGET:INTENSITY\n" sent over UART
// ---------------------------------------------------------------------------

pub struct SerialActuator {
    id: String,
    caps: Vec<String>,
    port: String,
    baud: u32,
}

impl SerialActuator {
    pub fn new(id: impl Into<String>, caps: Vec<String>, port: impl Into<String>, baud: u32) -> Self {
        Self { id: id.into(), caps, port: port.into(), baud }
    }
}

impl Actuator for SerialActuator {
    fn id(&self) -> &str { &self.id }
    fn capabilities(&self) -> &[String] { &self.caps }

    fn execute(&self, cmd: &ActuatorCommand) -> Result<ActuatorResult> {
        use std::io::Write;
        let payload = format!("{}:{}:{:.2}\n", cmd.action.to_uppercase(), cmd.target, cmd.intensity);
        match serialport::new(&self.port, self.baud)
            .timeout(std::time::Duration::from_millis(300))
            .open()
        {
            Ok(mut port) => {
                port.write_all(payload.as_bytes())?;
                println!("[caesar.{}.serial] Sent '{}' over {}", cmd.domain, payload.trim(), self.port);
                Ok(ActuatorResult {
                    actuator_id: self.id.clone(),
                    success: true,
                    message: format!("UART → {}: {}", self.port, payload.trim()),
                })
            }
            Err(e) => {
                eprintln!("[caesar.actuator.serial] Port {} error: {e}", self.port);
                Ok(ActuatorResult {
                    actuator_id: self.id.clone(),
                    success: false,
                    message: format!("Serial port {} unavailable: {e}", self.port),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MqttActuator — publishes commands to an MQTT broker over plain TCP
// Uses a hand-rolled MQTT 3.1.1 CONNECT + PUBLISH to keep dependencies minimal.
// ---------------------------------------------------------------------------

pub struct MqttActuator {
    id: String,
    caps: Vec<String>,
    broker_addr: String,   // host:port, e.g. "localhost:1883"
    topic: String,
}

impl MqttActuator {
    pub fn new(
        id: impl Into<String>,
        caps: Vec<String>,
        broker_addr: impl Into<String>,
        topic: impl Into<String>,
    ) -> Self {
        Self { id: id.into(), caps, broker_addr: broker_addr.into(), topic: topic.into() }
    }

    /// Encode a minimal MQTT 3.1.1 CONNECT packet (clean session, no auth).
    fn connect_packet(client_id: &str) -> Vec<u8> {
        let client_id_bytes = client_id.as_bytes();
        let payload_len = 2 + client_id_bytes.len();
        // Variable header: protocol name (MQTT) + level (4) + flags (0x02=CleanSession) + keepalive (60)
        let var_header: &[u8] = &[0x00, 0x04, b'M', b'Q', b'T', b'T', 0x04, 0x02, 0x00, 0x3C];
        let remaining_len = var_header.len() + payload_len;
        let mut pkt = vec![0x10u8, remaining_len as u8];
        pkt.extend_from_slice(var_header);
        pkt.push((client_id_bytes.len() >> 8) as u8);
        pkt.push(client_id_bytes.len() as u8);
        pkt.extend_from_slice(client_id_bytes);
        pkt
    }

    /// Encode a minimal MQTT 3.1.1 PUBLISH packet (QoS 0, no retain).
    fn publish_packet(topic: &str, payload: &[u8]) -> Vec<u8> {
        let topic_bytes = topic.as_bytes();
        let remaining_len = 2 + topic_bytes.len() + payload.len();
        let mut pkt = vec![0x30u8]; // PUBLISH, QoS=0
        // Remaining length (variable-length encoding, capped at 2 bytes for practical topic+payload sizes)
        if remaining_len < 128 {
            pkt.push(remaining_len as u8);
        } else {
            pkt.push(((remaining_len & 0x7F) | 0x80) as u8);
            pkt.push((remaining_len >> 7) as u8);
        }
        pkt.push((topic_bytes.len() >> 8) as u8);
        pkt.push(topic_bytes.len() as u8);
        pkt.extend_from_slice(topic_bytes);
        pkt.extend_from_slice(payload);
        pkt
    }
}

impl Actuator for MqttActuator {
    fn id(&self) -> &str { &self.id }
    fn capabilities(&self) -> &[String] { &self.caps }

    fn execute(&self, cmd: &ActuatorCommand) -> Result<ActuatorResult> {
        use std::io::Write;
        let payload = serde_json::json!({
            "action":     cmd.action,
            "target":     cmd.target,
            "intensity":  cmd.intensity,
            "confidence": cmd.confidence,
            "domain":     cmd.domain,
            "rationale":  cmd.rationale,
        }).to_string();

        let client_id = format!("caesar-edge-{}", &self.id[..self.id.len().min(10)]);
        let connect = Self::connect_packet(&client_id);
        let publish = Self::publish_packet(&self.topic, payload.as_bytes());

        match std::net::TcpStream::connect(&self.broker_addr) {
            Ok(mut stream) => {
                stream.set_write_timeout(Some(std::time::Duration::from_millis(500)))?;
                stream.write_all(&connect)?;
                stream.write_all(&publish)?;
                println!(
                    "[caesar.{}.mqtt] Published to {}/{} — action='{}'",
                    cmd.domain, self.broker_addr, self.topic, cmd.action
                );
                Ok(ActuatorResult {
                    actuator_id: self.id.clone(),
                    success: true,
                    message: format!("MQTT → {}/{}: {}", self.broker_addr, self.topic, cmd.action),
                })
            }
            Err(e) => {
                eprintln!("[caesar.actuator.mqtt] Broker {} unreachable: {e}", self.broker_addr);
                Ok(ActuatorResult {
                    actuator_id: self.id.clone(),
                    success: false,
                    message: format!("MQTT broker {} unavailable: {e}", self.broker_addr),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HttpWebhookActuator — POST a JSON payload to any HTTP endpoint
// Use case: Telegram bot alerts, PagerDuty, custom REST receivers,
// Home Assistant webhooks, Grafana OnCall, etc.
// Uses a hand-rolled HTTP/1.1 POST over TcpStream (no reqwest dependency).
// ---------------------------------------------------------------------------

pub struct HttpWebhookActuator {
    id: String,
    caps: Vec<String>,
    /// Full URL, e.g. "http://192.168.1.50:8080/webhook" or "https://api.example.com/alert"
    url: String,
    /// Optional static bearer token sent in Authorization header.
    bearer_token: Option<String>,
}

impl HttpWebhookActuator {
    pub fn new(
        id: impl Into<String>,
        caps: Vec<String>,
        url: impl Into<String>,
        bearer_token: Option<String>,
    ) -> Self {
        Self { id: id.into(), caps, url: url.into(), bearer_token }
    }

    /// Parse the host:port and path out of a plain http:// URL.
    /// Returns (host_port, path). HTTPS is not supported without TLS — use an
    /// nginx/caddy reverse proxy with TLS termination for secure endpoints.
    fn parse_http_url(url: &str) -> Option<(String, String)> {
        let stripped = url.strip_prefix("http://")?;
        let slash = stripped.find('/').unwrap_or(stripped.len());
        let host_port = stripped[..slash].to_string();
        let path = if slash < stripped.len() { stripped[slash..].to_string() } else { "/".to_string() };
        // Default port 80 if not specified
        let host_port = if host_port.contains(':') {
            host_port
        } else {
            format!("{}:80", host_port)
        };
        Some((host_port, path))
    }
}

impl Actuator for HttpWebhookActuator {
    fn id(&self) -> &str { &self.id }
    fn capabilities(&self) -> &[String] { &self.caps }

    fn execute(&self, cmd: &ActuatorCommand) -> Result<ActuatorResult> {
        use std::io::{Read, Write};
        let body = serde_json::json!({
            "action":     cmd.action,
            "target":     cmd.target,
            "intensity":  cmd.intensity,
            "confidence": cmd.confidence,
            "domain":     cmd.domain,
            "rationale":  cmd.rationale,
        }).to_string();

        let Some((host_port, path)) = Self::parse_http_url(&self.url) else {
            // HTTPS or unparseable — log and skip gracefully
            eprintln!("[caesar.actuator.webhook] Unsupported URL scheme (use http://): {}", self.url);
            return Ok(ActuatorResult {
                actuator_id: self.id.clone(),
                success: false,
                message: format!("Unsupported URL scheme: {}", self.url),
            });
        };

        let host = host_port.split(':').next().unwrap_or(&host_port).to_string();
        let mut request = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        if let Some(token) = &self.bearer_token {
            request.push_str(&format!("Authorization: Bearer {}\r\n", token));
        }
        request.push_str("\r\n");
        request.push_str(&body);

        match std::net::TcpStream::connect(&host_port) {
            Ok(mut stream) => {
                stream.set_write_timeout(Some(std::time::Duration::from_millis(2000)))?;
                stream.set_read_timeout(Some(std::time::Duration::from_millis(2000)))?;
                stream.write_all(request.as_bytes())?;
                // Read just the status line to confirm delivery
                let mut buf = [0u8; 64];
                let _ = stream.read(&mut buf);
                let status = String::from_utf8_lossy(&buf);
                let success = status.contains("200") || status.contains("201") || status.contains("204") || status.contains("No Content");
                println!(
                    "[caesar.{}.webhook] POST {} → action='{}' status_hint='{}'",
                    cmd.domain, self.url, cmd.action, status.lines().next().unwrap_or("").trim()
                );
                Ok(ActuatorResult {
                    actuator_id: self.id.clone(),
                    success,
                    message: format!("HTTP POST {} → {}", self.url, cmd.action),
                })
            }
            Err(e) => {
                eprintln!("[caesar.actuator.webhook] Cannot reach {}: {e}", host_port);
                Ok(ActuatorResult {
                    actuator_id: self.id.clone(),
                    success: false,
                    message: format!("Webhook endpoint {} unreachable: {e}", self.url),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GpioActuator — direct Linux sysfs GPIO toggle (Pi, embedded Linux)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
pub struct GpioActuator {
    id: String,
    caps: Vec<String>,
    pin: u32,
}

#[cfg(target_os = "linux")]
impl GpioActuator {
    pub fn new(id: impl Into<String>, caps: Vec<String>, pin: u32) -> Self {
        Self { id: id.into(), caps, pin }
    }
}

#[cfg(target_os = "linux")]
impl Actuator for GpioActuator {
    fn id(&self) -> &str { &self.id }
    fn capabilities(&self) -> &[String] { &self.caps }

    fn execute(&self, cmd: &ActuatorCommand) -> Result<ActuatorResult> {
        let value = if cmd.intensity > 0.5 { "1" } else { "0" };
        let path = format!("/sys/class/gpio/gpio{}/value", self.pin);
        std::fs::write(&path, value)?;
        println!("[caesar.{}.gpio] Pin {} → {} ({})", cmd.domain, self.pin, value, cmd.action);
        Ok(ActuatorResult {
            actuator_id: self.id.clone(),
            success: true,
            message: format!("GPIO pin {} → {} for '{}'", self.pin, value, cmd.action),
        })
    }
}

// ---------------------------------------------------------------------------
// ActuatorBus — routes commands to capable actuators
// ---------------------------------------------------------------------------

pub struct ActuatorBus {
    actuators: Vec<Box<dyn Actuator>>,
    pub autonomous_confidence_threshold: f32,
    pub autonomous_snapshot_threshold: usize,
}

impl ActuatorBus {
    pub fn from_config(
        configs: &[ActuatorConfig],
        autonomous_confidence: f32,
        autonomous_snapshots: usize,
    ) -> Self {
        let mut bus = Self {
            actuators: Vec::new(),
            autonomous_confidence_threshold: autonomous_confidence,
            autonomous_snapshot_threshold: autonomous_snapshots,
        };

        // System log actuator is always registered — every action gets an audit trail
        bus.register(Box::new(LogActuator::new("system_log", vec![
            "alert".into(), "increase_flow".into(), "decrease_flow".into(),
            "stop_flow".into(), "maintenance_alert".into(), "lock_perimeter".into(),
            "shutdown_zone".into(), "activate_irrigation".into(),
        ])));

        for cfg in configs {
            match cfg.actuator_type.as_str() {
                "log" => {
                    bus.register(Box::new(LogActuator::new(&cfg.id, cfg.capabilities.clone())));
                }
                "serial" => {
                    if let (Some(port), Some(baud)) = (&cfg.serial_port, cfg.baud_rate) {
                        bus.register(Box::new(SerialActuator::new(
                            &cfg.id, cfg.capabilities.clone(), port.as_str(), baud,
                        )));
                    } else {
                        eprintln!("[actuator] serial actuator '{}' missing serial_port or baud_rate", cfg.id);
                    }
                }
                #[cfg(target_os = "linux")]
                "gpio" => {
                    if let Some(pin) = cfg.gpio_pin {
                        bus.register(Box::new(GpioActuator::new(&cfg.id, cfg.capabilities.clone(), pin)));
                    } else {
                        eprintln!("[actuator] gpio actuator '{}' missing gpio_pin", cfg.id);
                    }
                }
                "mqtt" => {
                    if let Some(topic) = &cfg.mqtt_topic {
                        // Broker address: use mqtt_broker field if present, else fallback to localhost:1883
                        let broker = cfg.mqtt_broker.as_deref().unwrap_or("localhost:1883");
                        bus.register(Box::new(MqttActuator::new(
                            &cfg.id, cfg.capabilities.clone(), broker, topic.as_str(),
                        )));
                    } else {
                        eprintln!("[actuator] mqtt actuator '{}' missing mqtt_topic", cfg.id);
                    }
                }
                "webhook" => {
                    if let Some(url) = &cfg.webhook_url {
                        bus.register(Box::new(HttpWebhookActuator::new(
                            &cfg.id, cfg.capabilities.clone(),
                            url.as_str(), cfg.webhook_bearer_token.clone(),
                        )));
                    } else {
                        eprintln!("[actuator] webhook actuator '{}' missing webhook_url", cfg.id);
                    }
                }
                other => eprintln!("[actuator] Unknown actuator type '{}' for id '{}'", other, cfg.id),
            }
        }
        bus
    }

    pub fn register(&mut self, actuator: Box<dyn Actuator>) {
        self.actuators.push(actuator);
    }

    /// Dispatch a command to every actuator capable of handling `cmd.action`.
    /// The `system_log` actuator always receives the command for audit purposes,
    /// even if no domain-specific actuator is registered for the action.
    pub fn dispatch(&self, cmd: ActuatorCommand) -> Vec<ActuatorResult> {
        let mut results = Vec::new();
        let mut dispatched_to_domain = false;

        for actuator in &self.actuators {
            if actuator.id() == "system_log" { continue; } // handled separately below
            if actuator.capabilities().iter().any(|c| c == &cmd.action) {
                dispatched_to_domain = true;
                match actuator.execute(&cmd) {
                    Ok(r) => results.push(r),
                    Err(e) => results.push(ActuatorResult {
                        actuator_id: actuator.id().to_string(),
                        success: false,
                        message: format!("Execution error: {e}"),
                    }),
                }
            }
        }

        // System log always gets the command — audit trail is non-negotiable
        if let Some(log) = self.actuators.iter().find(|a| a.id() == "system_log") {
            if let Ok(r) = log.execute(&cmd) {
                results.push(r);
            }
        }

        if !dispatched_to_domain {
            eprintln!(
                "[caesar.actuator] No domain actuator found for action '{}'. Logged only.",
                cmd.action
            );
        }

        results
    }
}

// ---------------------------------------------------------------------------
// Domain-aware action inference
// ---------------------------------------------------------------------------

/// Translates an AI class label into a concrete actuator action,
/// informed by the Project Caesar deployment domain.
///
/// This is the heuristic translation layer between AI classification
/// and physical/logical system response. Operators can extend this
/// function as new class labels are added to the vocabulary.
pub fn infer_action_from_class(class: &str, domain: &str) -> String {
    let c = class.to_lowercase();
    match domain {
        "agricultural" => {
            if c.contains("growth") || c.contains("plant") || c.contains("crop") || c.contains("dry") || c.contains("wilt") {
                "increase_flow".into()
            } else if c.contains("flood") || c.contains("saturated") || c.contains("overwater") {
                "decrease_flow".into()
            } else if c.contains("pest") || c.contains("disease") || c.contains("stress") {
                "alert".into()
            } else {
                "alert".into()
            }
        }
        "industrial" => {
            if c.contains("overheat") || c.contains("fire") || c.contains("smoke") {
                "maintenance_alert".into()
            } else if c.contains("leak") || c.contains("spill") || c.contains("rupture") {
                "shutdown_zone".into()
            } else {
                "maintenance_alert".into()
            }
        }
        "tactical" => {
            if c.contains("intruder") || c.contains("armed") || c.contains("drone") || c.contains("uav") {
                "lock_perimeter".into()
            } else {
                "alert".into()
            }
        }
        _ => "alert".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_action_from_class() {
        // Agricultural
        assert_eq!(infer_action_from_class("crop_stress", "agricultural"), "alert");
        assert_eq!(infer_action_from_class("dry_soil", "agricultural"), "increase_flow");
        assert_eq!(infer_action_from_class("flooded_field", "agricultural"), "decrease_flow");
        
        // Industrial
        assert_eq!(infer_action_from_class("smoke_detected", "industrial"), "maintenance_alert");
        assert_eq!(infer_action_from_class("chemical_leak", "industrial"), "shutdown_zone");
        
        // Tactical
        assert_eq!(infer_action_from_class("armed_intruder", "tactical"), "lock_perimeter");
        assert_eq!(infer_action_from_class("civilian", "tactical"), "alert");
        
        // General fallback
        assert_eq!(infer_action_from_class("unknown_object", "general"), "alert");
    }

    #[test]
    fn test_actuator_bus_dispatch() {
        let mut bus = ActuatorBus {
            actuators: Vec::new(),
            autonomous_confidence_threshold: 0.85,
            autonomous_snapshot_threshold: 6,
        };
        
        // Register standard log actuator
        bus.register(Box::new(LogActuator::new("system_log", vec!["alert".into(), "shutdown_zone".into()])));
        
        let cmd = ActuatorCommand {
            action: "shutdown_zone".into(),
            target: "zone_alpha".into(),
            intensity: 0.9,
            rationale: "Testing dispatch".into(),
            confidence: 0.95,
            domain: "industrial".into(),
        };
        
        let results = bus.dispatch(cmd);
        assert_eq!(results.len(), 1); // Only system_log is registered
        assert_eq!(results[0].actuator_id, "system_log");
        assert!(results[0].success);
    }
}
