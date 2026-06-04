# Project Caesar — Linux Deployment & Hardware Hookup Guide

This guide covers the complete process for deploying Project Caesar on a Linux environment,
physically connecting hardware sensors and actuators, and operating the system end-to-end.

---

## Table of Contents

1. [System Prerequisites](#1-system-prerequisites)
2. [Compiling the Codebase](#2-compiling-the-codebase)
3. [Configuring the Hub](#3-configuring-the-hub)
4. [Configuring the Edge Node](#4-configuring-the-edge-node)
5. [Node Identity & Trust](#5-node-identity--trust)
6. [Connecting Hardware Devices (Actuators)](#6-connecting-hardware-devices-actuators)
   - [6.1 GPIO Relay](#61-gpio-relay---direct-physical-control)
   - [6.2 UART Serial Device](#62-uart-serial-device---microcontroller-effectors)
   - [6.3 MQTT Broker](#63-mqtt-broker---smart-devices--iot-peripherals)
   - [6.4 HTTP Webhook](#64-http-webhook---any-rest-endpoint)
7. [Connecting a Camera](#7-connecting-a-camera)
8. [Sensor Adapters (Thermal & Radar)](#8-sensor-adapters-thermal--radar)
9. [Launching the Dashboard Console](#9-launching-the-dashboard-console)
10. [Running as Systemd Services](#10-running-as-systemd-services)
11. [How AI Detections Trigger Physical Actions](#11-how-ai-detections-trigger-physical-actions)
12. [Full Pi 3 Hybrid Deployment](#12-full-pi-3-hybrid-deployment)

---

## 1. System Prerequisites

### Hub PC (x86-64 Linux)

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libclang-dev \
    cmake \
    v4l-utils \
    python3-pip \
    python3-venv \
    git \
    curl \
    mosquitto \
    mosquitto-clients

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup update stable

# Install Ollama (VLM for optical inference)
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llava

# Python environment for hub services
python3 -m venv .venv-console
source .venv-console/bin/activate
pip install paho-mqtt opencv-python pillow requests

mkdir -p output/caesar
```

> **Or use the bootstrap script:**
> ```bash
> bash scripts/bootstrap_hub.sh /opt/uriel-caesar
> ```

### Raspberry Pi 3 (ARM Linux / Pi OS Bookworm)

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libclang-dev \
    libopencv-dev \
    libpcap-dev \
    mosquitto \
    mosquitto-clients \
    python3 \
    python3-pip \
    python3-venv \
    git \
    curl \
    ffmpeg \
    python3-smbus \
    i2c-tools \
    rpicam-apps

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Python environment for sensor adapters
python3 -m venv .venv-edge
source .venv-edge/bin/activate
pip install -r requirements-edge.txt
# requirements-edge.txt includes: numpy, onnxruntime, Pillow, pyserial, smbus2, pynacl
```

> **Or use the bootstrap script:**
> ```bash
> bash scripts/bootstrap_edge_pi.sh /opt/uriel-caesar
> ```

> **Pi 3 Note:** `libopencv-dev` and `libclang-dev` are required to compile the `opencv`
> crate even when `sentinel.enabled = false`. Without them, `cargo build` fails at
> the `opencv` build script.

---

## 2. Compiling the Codebase

### Hub PC

```bash
# Compile only the hub binary (fast — no OpenCV dependency)
cargo build --release -p caesar-hub

# Or compile everything (requires libopencv-dev on the build machine)
cargo build --release
```

### Raspberry Pi 3

```bash
# Native build on the Pi (slow ~15-20 min; OpenCV must be installed)
cargo build --release -p uriel-edge-node

# Cross-compile from an x86 PC (much faster):
# Install cross: cargo install cross
# cross build --release --target aarch64-unknown-linux-gnu -p uriel-edge-node
```

Binaries are at:
- **Hub:** `target/release/caesar-hub`
- **Edge Node:** `target/release/uriel-edge-node`

> **Pi 3 inference note:** In `remote_http` mode the Pi 3 loads **zero ONNX model weights**.
> All AI inference runs on the hub PC. Pi 3 RAM footprint is under 50 MB.

---

## 3. Configuring the Hub

### `configs/hub-dev.toml`

```toml
# Bind address for incoming edge node connections
listen_addr = "127.0.0.1:7878"

# Public key allowlist — add each edge node's fingerprint here.
# On first boot each edge node logs its public key fingerprint.
# Copy it here to enable the allowlist.
# Leave empty [] (or omit) to accept any node (development only).
trusted_public_keys = [
    "paste-node-public-key-hex-fingerprint-here",
]

[storage]
journal_path       = "output/caesar/journal.jsonl"      # all envelopes, append-only
latest_path        = "output/caesar/latest_tracks.json" # latest per track_id, overwritten
high_interest_path = "output/caesar/high_interest.jsonl" # threat_level=high-interest only
```

```bash
mkdir -p output/caesar
```

### Launch the Hub

```bash
./target/release/caesar-hub --config configs/hub-dev.toml serve

# Print the latest track snapshot to stdout:
./target/release/caesar-hub --config configs/hub-dev.toml latest
```

---

## 4. Configuring the Edge Node

All configuration lives in a TOML file. Use `configs/edge-pi3.toml` for Pi 3,
`configs/edge-dev.toml` for development (all synthetic data).

### Core Fields

```toml
node_id          = "tower-bwari-pi3"     # Unique name for this node
publish_topic    = "caesar_tactical_intel"
loop_count       = 0                      # 0 = run forever
fusion_window_ms = 700                    # Observation collection window (ms)
threat_threshold = 0.75                   # Confidence above which tracks are high-interest
recon_enabled    = true
domain           = "tactical"             # "agricultural"|"industrial"|"tactical"|"general"

# Key management: auto-generated on first boot, no manual action needed.
# The node generates a unique Ed25519 keypair from OS entropy and persists it here.
key_file = "/etc/caesar/tower-bwari-pi3.key"

# DO NOT set ed25519_seed_hex in production — remove it if present.
# It is a legacy migration path only (used once to populate the key file, then remove it).

[location]
site      = "Bwari"
latitude  = 9.2797
longitude = 7.3781
```

### Uplink Configuration

```toml
[uplink]
# Primary: stream signed envelopes to the hub over TCP
mode     = "tcp_jsonl"
tcp_addr = "10.163.194.96:7878"   # ← Hub PC LAN IP

# Alternative: UART radio link (RFD900x / LoRa)
# mode      = "uart"
# uart_port = "/dev/ttyUSB0"
# baud_rate = 57600

# Alternative: log to local file (offline / debug)
# mode      = "file"
# file_path = "output/tracks.jsonl"
```

### Inference Configuration

```toml
[inference]
# remote_http: offload optical AI to the hub (Pi 3 mode — zero weights on device)
mode            = "remote_http"
ollama_endpoint = "http://10.163.194.96:9090"   # ← Same hub IP
ollama_model    = "llava"

# ort_native: run YOLO-World ONNX + Ollama locally (for devices with more RAM/GPU)
# mode             = "ort_native"
# model_yolo_world = "models/yolo_world_v2_s.onnx"
# ollama_endpoint  = "http://localhost:11434"

vocabulary = [
    "human intruder", "armed person", "civilian drone", "military drone",
    "tractor", "truck", "motorcycle", "car", "animal",
    "fire", "smoke", "explosion", "weapon", "backpack",
    "person running", "person crouching", "crowd", "vehicle convoy"
]
```

### Sentinel Pipeline (OpenCV)

Enable when a USB or CSI camera is physically attached:

```toml
[sentinel]
enabled               = true    # Flip this on with a real camera
device_id             = 0       # /dev/video0 → 0, /dev/video1 → 1
fps                   = 24
motion_area_threshold = 500.0   # Minimum contour area in px² for motion
anomaly_threshold     = 0.30    # Accumulate snapshots above this score
burst_threshold       = 0.70    # Skip accumulation, escalate AI immediately
min_snapshots         = 8       # Require this many confirmed frames
autonomous_confidence = 0.87    # Actuator fires only above this mean confidence
snapshot_dir          = "data/snapshots"   # Optional: persist JPEG crops
mjpeg_port            = 8091               # Optional: live camera stream port
```

> When `sentinel.enabled = true`, the standard optical worker is disabled —
> the sentinel owns the camera exclusively. Set `sentinel.enabled = false` on Pi 3
> to use the standard optical worker (lower RAM, no OpenCV dependency at runtime).

---

## 5. Node Identity & Trust

### How it works

- On **first boot**, `NodeIdentity::load_or_generate()` uses `OsRng` (OS entropy) to
  generate a fresh Ed25519 keypair. The seed is written to `key_file` with `chmod 600`.
- On **subsequent boots**, the seed is loaded from the file — the identity is stable.
- Every `FusedTrack` is **Ed25519 signed** before TCP transmission.
- The hub verifies signatures before storing any envelope.
- Unknown public keys are **logged and skipped** (the connection stays open).

### First-boot procedure

```bash
# 1. Create the key directory on the Pi:
sudo mkdir -p /etc/caesar
sudo chown pi:pi /etc/caesar

# 2. Launch the edge node. It will log:
#   [caesar.identity] Node 'tower-bwari-pi3' identity ready
#   ├─ Key file   : /etc/caesar/tower-bwari-pi3.key
#   ├─ Public key : <64-hex-char fingerprint>
#   └─ Add this public key to hub-dev.toml trusted_public_keys if not already present.

# 3. Copy the fingerprint into configs/hub-dev.toml:
trusted_public_keys = ["<paste fingerprint here>"]

# 4. Restart the hub to pick up the new key.
```

### Key file backup

```bash
# The key file is the node's permanent identity.
# Back it up — if lost, the hub will reject envelopes from this node
# until the new fingerprint is added to trusted_public_keys.
scp pi@<PI_IP>:/etc/caesar/tower-bwari-pi3.key ./backup/
```

---

## 6. Connecting Hardware Devices (Actuators)

When AI detects a threat with sufficient confidence, `ActuatorBus::dispatch()` routes a command
to every registered actuator capable of handling that action.

**Pipeline:** AI class label → `infer_action_from_class()` → action string → dispatched to actuators

### 6.1 GPIO Relay — Direct Physical Control

**Use case:** Lock/unlock a gate, activate a siren, trigger a floodlight, open a valve.

**Hardware wiring:**
1. Connect a 5V relay module to a Raspberry Pi GPIO pin (e.g., GPIO 17 = physical pin 11).
2. Wire your load to the relay's NO (Normally Open) terminal.
3. Use a transistor/opto-isolator for loads over 40mA.

> **GPIO backend:** The `gpio` actuator uses `rppal` (the modern character device interface)
> on Linux ≥5.10 (Pi OS Bookworm). The deprecated sysfs `/sys/class/gpio/` interface is
> **not used** — no manual `echo 17 > /sys/class/gpio/export` step is required.

**Permissions:**

```bash
# Add your user to the gpio group (Pi OS Bookworm)
sudo usermod -aG gpio $USER
# Log out and back in for this to take effect
```

**`edge-pi3.toml` configuration:**

```toml
[[actuators]]
id            = "perimeter_relay_north"
actuator_type = "gpio"
capabilities  = ["lock_perimeter", "alert"]
gpio_pin      = 17

[[actuators]]
id            = "alarm_siren"
actuator_type = "gpio"
capabilities  = ["alert", "maintenance_alert"]
gpio_pin      = 27
```

**What happens:** `armed_intruder` in `tactical` domain → `infer_action_from_class()` → `"lock_perimeter"` → `GpioActuator` drives GPIO17 HIGH, energising the relay.

---

### 6.2 UART Serial Device — Microcontroller Effectors

**Use case:** Control an Arduino, ESP32, or custom MCU over a serial connection.

**Hardware wiring:**
1. Use a USB-UART adapter (shows up as `/dev/ttyUSB0`).
2. If connecting directly to Pi GPIO UART: TX→RX cross-wired, GND→GND.
   Do **not** connect 5V — use a level shifter if the MCU runs at 5V.

**Arduino firmware example:**

```cpp
// Listens for Caesar commands: "ACTION:TARGET:INTENSITY\n"
void setup() { Serial.begin(9600); }

void loop() {
    if (Serial.available()) {
        String cmd = Serial.readStringUntil('\n');
        cmd.trim();
        if (cmd.startsWith("INCREASE_FLOW"))  openValve();
        else if (cmd.startsWith("STOP_FLOW")) closeValve();
        else if (cmd.startsWith("ALERT"))      activateIndicator();
        else if (cmd.startsWith("LOCK_PERIMETER")) lockGate();
    }
}
```

**Find your serial port:**

```bash
ls /dev/ttyUSB* /dev/ttyACM*
# USB-UART → /dev/ttyUSB0
# Arduino native USB → /dev/ttyACM0
# Pi UART → /dev/ttyAMA0 or /dev/serial0
```

**Permissions:**

```bash
sudo usermod -aG dialout $USER
# Log out and back in for this to take effect
```

**`edge-pi3.toml` configuration:**

```toml
[[actuators]]
id            = "irrigation_zone_3"
actuator_type = "serial"
capabilities  = ["increase_flow", "decrease_flow", "stop_flow", "activate_irrigation"]
serial_port   = "/dev/ttyUSB0"
baud_rate     = 9600

[[actuators]]
id            = "industrial_shutoff_zone_a"
actuator_type = "serial"
capabilities  = ["shutdown_zone", "maintenance_alert"]
serial_port   = "/dev/ttyACM0"
baud_rate     = 115200
```

---

### 6.3 MQTT Broker — Smart Devices & IoT Peripherals

**Use case:** Control MQTT-capable devices — ESPHome nodes, Zigbee bridges (via Mosquitto),
Home Assistant automations, industrial PLCs with MQTT adapters.

**Start the local broker (installed by bootstrap):**

```bash
sudo systemctl enable mosquitto
sudo systemctl start mosquitto
sudo systemctl status mosquitto

# Test it:
mosquitto_sub -h localhost -t "caesar/actions" -v &
mosquitto_pub -h localhost -t "caesar/actions" -m '{"action":"alert","target":"auto"}'
```

**ESPHome device configuration:**

```yaml
mqtt:
  broker: 10.163.194.96   # IP of the machine running Mosquitto
  on_message:
    - topic: caesar/actions
      then:
        - lambda: |-
            // JSON payload: {"action":"lock_perimeter","target":"auto","intensity":0.94,...}
            auto doc = cJSON_Parse(x.c_str());
            std::string action = cJSON_GetObjectItem(doc, "action")->valuestring;
            if (action == "lock_perimeter") id(gate_relay).turn_on();
            else if (action == "alert")     id(siren_relay).turn_on();
```

**`edge-pi3.toml` configuration:**

```toml
[[actuators]]
id            = "mqtt_command_bus"
actuator_type = "mqtt"
capabilities  = ["alert", "lock_perimeter", "maintenance_alert", "shutdown_zone"]
mqtt_topic    = "caesar/actions"
mqtt_broker   = "10.163.194.96:1883"
```

**JSON payload format sent on every dispatch:**

```json
{
  "action":     "lock_perimeter",
  "target":     "auto",
  "intensity":  0.94,
  "confidence": 0.91,
  "domain":     "tactical",
  "rationale":  "[caesar.tactical] AI detected 'armed-intruder' via ollama over 8 snapshots."
}
```

---

### 6.4 HTTP Webhook — Any REST Endpoint

**Use case:** Telegram bot alerts, PagerDuty, Home Assistant, n8n automations, or any custom HTTP listener.

```toml
[[actuators]]
id                   = "webhook_alert"
actuator_type        = "webhook"
capabilities         = ["alert", "lock_perimeter", "maintenance_alert", "shutdown_zone"]
webhook_url          = "http://192.168.1.50:8080/caesar-alert"
# webhook_bearer_token = "your-secret-token"   # optional Authorization: Bearer header
```

The actuator POSTs the same JSON payload as the MQTT actuator above.

---

## 7. Connecting a Camera

### USB Webcam (any Linux)

```bash
# Verify the device is available
ls /dev/video*
v4l2-ctl --list-devices

# Set in edge config:
[sentinel]
enabled   = true
device_id = 0    # /dev/video0
```

### Raspberry Pi CSI Camera (libcamera / rpicam)

```bash
# Pi OS Bookworm — no raspi-config needed, just verify:
rpicam-hello --timeout 2000   # should show a preview for 2 seconds
```

The CSI camera is used by the **standard optical worker** (not the sentinel) on Pi 3.
The worker calls `rpicam-jpeg` via the `profile_stdout` optical mode, which is already
configured in `configs/edge-pi3.toml`.

```toml
[optical]
enabled           = true
mode              = "profile_stdout"
profile           = "rpi_csi_jpeg"
camera_id         = "pi3-cam-front"
width             = 640
height            = 480
frame_interval_ms = 1000    # 1fps to minimise hub upload bandwidth
csi_port          = 0
```

> **Sentinel on Pi 3:** Keep `sentinel.enabled = false` on Pi 3. The sentinel uses
> OpenCV VideoCapture which competes with `rpicam-jpeg`. Use the standard optical
> worker + `remote_http` inference instead.

---

## 8. Sensor Adapters (Thermal & Radar)

### Thermal: MLX90640 (I2C)

**Wiring (Pi 3):**
- VCC → 3.3V (pin 1)
- GND → GND (pin 6)
- SDA → GPIO2 / SDA1 (pin 3)
- SCL → GPIO3 / SCL1 (pin 5)

```bash
# Enable I2C
sudo raspi-config  # → Interface Options → I2C → Enable
# Verify sensor is detected
i2cdetect -y 1    # should show device at 0x33
```

**Test the adapter:**

```bash
source .venv-edge/bin/activate
python scripts/thermal_adapter.py --mode hardware --bus 1 --address 0x33
# Should output: {"timestamp_ms": ..., "temperatures_c": [...768 values...]}

# Synthetic test (no hardware):
python scripts/thermal_adapter.py --mode synthetic
```

**`edge-pi3.toml`:**

```toml
[thermal]
enabled           = true
mode              = "command_json"
camera_id         = "thermal-front"
width             = 32
height            = 24
frame_interval_ms = 600
command_program   = "python3"
command_args      = ["scripts/thermal_adapter.py", "--mode", "hardware",
                     "--bus", "1", "--address", "0x33"]
```

### Radar: HLK-LD2450 / TI IWR6843 (UART)

**Wiring:** Connect the LD2450 UART to Pi's `/dev/ttyAMA0`.

```bash
# Disable Pi serial console (to free ttyAMA0 for radar)
sudo raspi-config  # → Interface Options → Serial Port → Login shell: No, Hardware: Yes

# Test the adapter:
source .venv-edge/bin/activate
python scripts/radar_adapter.py --mode hardware --port /dev/ttyAMA0 --baud 256000
# Should output: {"timestamp_ms": ..., "points": [...]}

# Synthetic test:
python scripts/radar_adapter.py --mode synthetic --points 48
```

**`edge-pi3.toml`:**

```toml
[radar]
enabled           = true
mode              = "command_json"
radar_id          = "mmwave-front"
point_count       = 64
frame_interval_ms = 550
command_program   = "python3"
command_args      = ["scripts/radar_adapter.py", "--mode", "hardware",
                     "--port", "/dev/ttyAMA0", "--baud", "256000"]
```

---

## 9. Launching the Dashboard Console

```bash
source .venv-console/bin/activate
python services/caesar_console/server.py --host 127.0.0.1 --port 8090

# Open in browser:
# http://127.0.0.1:8090
```

The dashboard provides:
- **Live track feed** — real-time fused observations per node
- **Actuator command panel** — dispatch manual commands to any node
- **MJPEG live feed** — when `sentinel.mjpeg_port` is configured on the node
- **Node health** — uplink status, last-seen timestamps
- **MQTT command forwarding** — dispatches commands to edge nodes via `caesar/commands/<node_id>`

---

## 10. Running as Systemd Services

### Hub Service

```bash
sudo nano /etc/systemd/system/caesar-hub.service
```

```ini
[Unit]
Description=Project Caesar Regional Hub
After=network.target

[Service]
Type=simple
User=pi
WorkingDirectory=/opt/uriel-caesar
ExecStart=/opt/uriel-caesar/target/release/caesar-hub --config configs/hub-dev.toml serve
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

### Edge Node Service

```bash
sudo nano /etc/systemd/system/uriel-edge.service
```

```ini
[Unit]
Description=Uriel Edge Node (Project Caesar)
After=network.target

[Service]
Type=simple
User=pi
WorkingDirectory=/opt/uriel-caesar
ExecStart=/opt/uriel-caesar/target/release/uriel-edge-node --config configs/edge-pi3.toml
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

### Remote Inference Server Service (Hub)

```bash
sudo nano /etc/systemd/system/caesar-remote-infer.service
```

```ini
[Unit]
Description=Project Caesar Remote Inference Server
After=network.target ollama.service

[Service]
Type=simple
User=pi
WorkingDirectory=/opt/uriel-caesar
ExecStart=/opt/uriel-caesar/.venv-console/bin/python services/remote_infer_server.py --host 10.163.194.96 --port 9090
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

### Enable & Start

```bash
sudo systemctl daemon-reload
sudo systemctl enable caesar-hub caesar-remote-infer uriel-edge
sudo systemctl start  caesar-hub caesar-remote-infer uriel-edge
sudo systemctl status caesar-hub caesar-remote-infer uriel-edge

# Follow logs:
journalctl -u caesar-hub -f
journalctl -u uriel-edge -f
```

---

## 11. How AI Detections Trigger Physical Actions

```
Camera / Radar / Thermal Sensors
    │
    ▼  Optical: OpenCV MOG2 background subtraction (sentinel mode)
       OR: rpicam-jpeg / thermal_adapter.py / radar_adapter.py (standard mode)
    │
    ▼  Observation emitted to FusionEngine
    │
    ▼  FusionEngine (EKF + temporal deduplication)
       - 700ms collection window
       - Timestamp-based EKF dt (not wall clock)
       - Deduplication via HashSet (O(n))
       - Per-track acoustic z-score anomaly detection
    │
    ▼  FusedTrack produced  (threat_level = "high-interest" OR "monitor")
    │
    ▼  Ed25519 signed → TCP stream → hub (caesar-hub)
    │  OR → local file / UART radio (uplink mode)
    │
    ▼  ALSO: If confidence ≥ autonomous_confidence (0.87):
    │
AI Inference Cascade:
    1. YOLO-World ONNX (zero-shot, local or hub)
    2. Ollama LLaVA VLM (visual reasoning)
    3. Heuristic fallback ("motion-detected" → "alert", safe neutral)
    │
    ▼  class_label → infer_action_from_class(label, domain)
    │
    ▼  ActuatorBus::dispatch(action)
    │
    ├─▶  GpioActuator    → rppal GPIO HIGH/LOW           (relay, siren, lock)
    ├─▶  SerialActuator  → UART "ACTION:TARGET:0.94\n"  (Arduino, ESP32, MCU)
    ├─▶  MqttActuator    → JSON to caesar/actions topic  (ESPHome, HomeAssistant)
    ├─▶  WebhookActuator → HTTP POST JSON                (Telegram, PagerDuty, n8n)
    └─▶  LogActuator     → Structured audit log          (always present)
```

### Domain → Action Reference

| Domain | AI Detects | Action |
|--------|-----------|--------|
| `tactical` | `armed-intruder`, `drone`, `uav` | `lock_perimeter` |
| `tactical` | anything else | `alert` |
| `agricultural` | `dry-soil`, `crop-wilt`, `plant-stress` | `increase_flow` |
| `agricultural` | `flood`, `saturated`, `overwater` | `decrease_flow` |
| `agricultural` | `pest`, `disease` | `alert` |
| `industrial` | `overheat`, `fire`, `smoke` | `maintenance_alert` |
| `industrial` | `leak`, `spill`, `rupture` | `shutdown_zone` |
| `general` | anything | `alert` |
| *any* (hub offline) | heuristic fallback | `alert` (safe neutral — no `lock_perimeter`) |

To extend the mapping, edit `infer_action_from_class()` in
`crates/uriel-edge-node/src/actuator.rs` and add capabilities to your actuator config.

---

## 12. Full Pi 3 Hybrid Deployment

Project Caesar runs at **100% functionality** on a Raspberry Pi 3 (1GB RAM) by offloading
AI compute to the hub PC. The Pi 3 handles all sensor capture, fusion, actuation, and signing;
the hub PC handles Ollama VLM inference and storage.

### Architecture

```
┌──────────────────── Raspberry Pi 3 ─────────────────────────────┐
│  rpicam-jpeg (CSI) → JPEG frames                                │
│  thermal_adapter.py (I2C MLX90640) → temperature grid          │
│  radar_adapter.py (UART LD2450) → point cloud                  │
│  EKF Sensor FusionEngine (per-track acoustic z-score)          │
│  GPIO / UART / MQTT / Webhook Actuators                        │
│  Passive Recon (async ONVIF, eBPF, PCAP, SoapySDR)            │
│  Ed25519 signing → TCP uplink                                  │
│                                                                 │
│  inference mode = "remote_http"                                 │
│    JPEG ──── POST /infer ───────────────────────────────────►  │
│         ◄─── Observation JSON (class, confidence) ──────────── │
└─────────────────────────────────────────────────────────────────┘
                │  TCP :7878  signed FusedTrack envelopes
                ▼
┌──────────────── Hub PC (e.g. 10.163.194.96) ────────────────────┐
│  caesar-hub         :7878  — receives, verifies, stores         │
│  remote_infer_server:9090  — Ollama VLM for optical + proxy    │
│  Ollama llava        :11434 — thermal/radar text prompts        │
│  caesar_console      :8090  — tactical dashboard               │
└─────────────────────────────────────────────────────────────────┘
```

> **Offline resilience:** If the hub becomes unreachable, the Pi 3 automatically falls back
> to the local heuristic engine. The heuristic returns `"motion-detected"` → `"alert"`
> (conservative — does not trigger `lock_perimeter`). Streaming resumes when the hub reconnects.

### Launch Sequence

#### On the Hub PC (three terminals):

```bash
# Terminal 1 — Hub: receive and store signed envelopes
./target/release/caesar-hub --config configs/hub-dev.toml serve

# Terminal 2 — Remote Inference Server: VLM for Pi 3 frames
source .venv-console/bin/activate
python services/remote_infer_server.py --host 10.163.194.96 --port 9090

# Terminal 3 — Dashboard
source .venv-console/bin/activate
python services/caesar_console/server.py --host 127.0.0.1 --port 8090
# Open http://localhost:8090
```

#### On the Pi 3:

```bash
# Build (no ONNX models needed for remote_http mode)
cargo build --release -p uriel-edge-node

# Create key directory (first time only)
sudo mkdir -p /etc/caesar && sudo chown pi:pi /etc/caesar

# Launch
./target/release/uriel-edge-node --config configs/edge-pi3.toml
```

**Expected first-boot output:**

```
[caesar] Booting Uriel edge node 'tower-bwari-pi3' | site='Bwari' | domain='tactical' | uplink='tcp_jsonl'
[caesar.identity] Generating new Ed25519 identity for node 'tower-bwari-pi3'
[caesar.identity] New identity key written to '/etc/caesar/tower-bwari-pi3.key'.
└─ Back this file up securely
[caesar.identity] Node 'tower-bwari-pi3' identity ready
├─ Key file   : /etc/caesar/tower-bwari-pi3.key
├─ Public key : <64-char-hex-fingerprint>   ← copy this to hub-dev.toml
└─ Add this public key to hub-dev.toml trusted_public_keys if not already present.
[caesar.command] MQTT broker resolved to 10.163.194.96:1883 (from config)
[edge.recon] Starting passive reconnaissance engine...
[caesar.tactical][remote_http] Inference OK — class='clear' conf=0.82 stage=ollama
```

### Feature Matrix

| Feature | Pi 5 / x86 (ort_native) | Pi 3 (remote_http) |
|---------|-------------------------|---------------------|
| Camera capture (CSI/USB) | ✅ | ✅ identical |
| Thermal I2C (MLX90640) | ✅ | ✅ identical |
| Radar UART (LD2450) | ✅ | ✅ identical |
| YOLO-World classification | ✅ local | ✅ hub (result returned) |
| Ollama VLM visual reasoning | ✅ local | ✅ hub (result returned) |
| Ollama text prompts (thermal/radar) | ✅ local | ✅ proxied via hub :9090 |
| Heuristic fallback (hub offline) | ✅ | ✅ auto-engaged |
| EKF Sensor Fusion | ✅ | ✅ identical |
| GPIO actuator | ✅ | ✅ identical |
| UART serial actuator | ✅ | ✅ identical |
| MQTT actuator | ✅ | ✅ identical |
| HTTP Webhook actuator | ✅ | ✅ identical |
| Passive Recon (async ONVIF, eBPF, PCAP) | ✅ | ✅ identical |
| Ed25519 envelope signing | ✅ | ✅ identical |
| Dashboard Console | ✅ | ✅ runs on hub |
| OpenCV Sentinel (MOG2) | ✅ | ❌ disabled (uses standard optical worker) |
| **Pi 3 RAM footprint** | >1 GB | **< 50 MB** |

### Verify the Remote Inference Server

```bash
# Health check
curl http://10.163.194.96:9090/health

# Manual inference test
python3 - <<'EOF'
import base64, json, urllib.request
jpeg = bytes([0xFF,0xD8,0xFF,0xE0,0x00,0x10,0x4A,0x46,0x49,0x46,0x00,0x01,
              0x01,0x00,0x00,0x01,0x00,0x01,0x00,0x00,0xFF,0xD9])
body = json.dumps({
    "jpeg_b64": base64.b64encode(jpeg).decode(),
    "domain": "tactical", "sequence": 1,
    "node_id": "test-pi3", "timestamp_ms": 1000
}).encode()
req = urllib.request.Request("http://10.163.194.96:9090/infer",
      data=body, headers={"Content-Type": "application/json"}, method="POST")
print(json.loads(urllib.request.urlopen(req).read()))
EOF
# Expected: {"track_hint": "ollama-track-1", "class_label": "clear", "confidence": 0.84, ...}
```
