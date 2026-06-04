# Project Caesar — Linux Deployment & Hardware Hookup Guide

This guide covers the complete process for deploying Project Caesar on a Linux environment and physically connecting hardware devices — cameras, GPIO relays, UART microcontrollers, and MQTT-controlled peripherals — that will act autonomously in response to AI detections.

---

## Table of Contents

1. [System Prerequisites](#1-system-prerequisites)
2. [Compiling the Codebase](#2-compiling-the-codebase)
3. [Configuring the Hub (Core)](#3-configuring-the-hub-core)
4. [Configuring the Edge Node](#4-configuring-the-edge-node)
5. [Connecting Hardware Devices (Actuators)](#5-connecting-hardware-devices-actuators)
   - [5.1 GPIO Relay (Raspberry Pi / Jetson)](#51-gpio-relay---direct-physical-control)
   - [5.2 UART Serial Device (Arduino / MCU)](#52-uart-serial-device---microcontroller-effectors)
   - [5.3 MQTT Broker (Smart Devices / Home Automation)](#53-mqtt-broker---smart-devices--iot-peripherals)
6. [Connecting a Camera](#6-connecting-a-camera)
7. [Launching the Dashboard Console](#7-launching-the-dashboard-console)
8. [Running as Systemd Services](#8-running-as-systemd-services)
9. [How AI Detections Trigger Physical Actions](#9-how-ai-detections-trigger-physical-actions)

---

## 1. System Prerequisites

### Install Core Build Tools & OpenCV

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install -y \
    build-essential \
    pkg-config \
    cmake \
    libopencv-dev \
    libssl-dev \
    v4l-utils \
    python3-pip \
    mosquitto \
    mosquitto-clients
```

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup update stable
```

### Install Ollama (VLM Fallback Pipeline)

```bash
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llava
```

---

## 2. Compiling the Codebase

Clone the repository, then build the entire workspace. On Linux, OpenCV resolves automatically through `pkg-config` — no manual environment variables required:

```bash
git clone <your-repo-url> project-caesar
cd project-caesar
cargo build --release
```

Compiled binaries will be at:
- **Hub:** `target/release/caesar-hub`
- **Edge Node:** `target/release/uriel-edge-node`

> **Raspberry Pi Note:** Compilation on a Pi 4/5 takes ~15-20 minutes due to limited RAM. You can cross-compile from an x86 machine using `cross` and the `aarch64-unknown-linux-gnu` target for faster builds.

---

## 3. Configuring the Hub (Core)

The Hub is the regional intelligence aggregator. Run it on your primary Linux server.

### 3.1 Hub Config (`configs/hub-dev.toml`)

```toml
bind_address = "0.0.0.0:7878"
data_dir     = "/var/lib/caesar/hub"
```

### 3.2 Start the Hub

```bash
./target/release/caesar-hub serve --config configs/hub-dev.toml
```

---

## 4. Configuring the Edge Node

All configuration lives in `configs/edge-dev.toml`. Edit this file for each physical edge device.

### 4.1 Core Node Identity

```toml
node_id          = "tower-bwari-alpha"   # Unique name for this node
publish_topic    = "caesar_tactical_intel"
loop_count       = 0                     # 0 = run forever
fusion_window_ms = 600
threat_threshold = 0.72
ed25519_seed_hex = "<your-32-byte-hex-seed>"
domain           = "tactical"            # "agricultural" | "industrial" | "tactical" | "general"

[location]
site      = "Bwari"
latitude  = 9.2797
longitude = 7.3781

[uplink]
mode     = "tcp_jsonl"
tcp_addr = "192.168.1.100:7878"  # IP of the machine running caesar-hub
```

### 4.2 Enable the Sentinel Pipeline

Set `sentinel.enabled = true` when a real camera is attached:

```toml
[sentinel]
enabled               = true    # FLIP THIS ON with a real camera
device_id             = 0       # /dev/video0 → 0, /dev/video1 → 1
fps                   = 24
motion_area_threshold = 500.0
anomaly_threshold     = 0.30
burst_threshold       = 0.70
min_snapshots         = 8
autonomous_confidence = 0.87
snapshot_dir          = "data/snapshots"
mjpeg_port            = 8090   # Serves live camera feed to the dashboard
```

---

## 5. Connecting Hardware Devices (Actuators)

This is the core integration layer. When the AI pipeline detects a threat with sufficient confidence, it calls `ActuatorBus::dispatch()`, which routes the command to every registered hardware device capable of handling that action.

**The mapping is:** AI class label → `infer_action_from_class()` → action string (e.g., `"lock_perimeter"`) → dispatched to all actuators with that capability.

### 5.1 GPIO Relay — Direct Physical Control

**Use case:** Lock/unlock a gate, activate a siren, trigger a floodlight, open/close a valve directly from a Pi GPIO pin.

**Hardware wiring:**
1. Connect a 5V relay module to a Raspberry Pi GPIO pin (e.g., GPIO 17 = physical pin 11).
2. Wire your load (siren, lock solenoid, pump relay) to the relay's NO (Normally Open) terminal.

**Linux GPIO setup:**

Before the first run, export the pin to the sysfs interface:
```bash
echo "17" | sudo tee /sys/class/gpio/export
echo "out" | sudo tee /sys/class/gpio/gpio17/direction
```

To make this persistent on boot, add it to `/etc/rc.local`:
```bash
echo "17" > /sys/class/gpio/export
echo "out" > /sys/class/gpio/gpio17/direction
```

**`edge-dev.toml` configuration:**

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

**What happens:** When the AI detects an `armed_intruder` in `tactical` domain, `infer_action_from_class()` returns `"lock_perimeter"`. The `ActuatorBus` finds `perimeter_relay_north` (which has `"lock_perimeter"` in its capabilities) and writes `1` to `/sys/class/gpio/gpio17/value`, energising the relay.

---

### 5.2 UART Serial Device — Microcontroller Effectors

**Use case:** Control an Arduino, ESP32, or any custom MCU that drives irrigation valves, motorised locks, conveyor stops, or other industrial effectors over a serial connection.

**Hardware wiring:**
1. Connect Arduino TX→RPi RX and RX→TX (cross-wired), GND→GND.
2. Do **not** connect 5V unless the Pi's UART is 5V tolerant (it isn't — use a level shifter).
3. Plug in via USB-UART adapter instead (shows up as `/dev/ttyUSB0`).

**Arduino firmware (example):**

```cpp
// Arduino sketch — listens for Caesar commands over Serial
void setup() { Serial.begin(9600); }

void loop() {
  if (Serial.available()) {
    String cmd = Serial.readStringUntil('\n');  // Format: "ACTION:TARGET:INTENSITY\n"
    cmd.trim();
    if (cmd.startsWith("INCREASE_FLOW")) {
      openValve();
    } else if (cmd.startsWith("STOP_FLOW")) {
      closeValve();
    } else if (cmd.startsWith("ALERT")) {
      activateIndicator();
    }
  }
}
```

**Find your serial port:**
```bash
ls /dev/ttyUSB* /dev/ttyACM*
# Typically: /dev/ttyUSB0 (USB-UART) or /dev/ttyACM0 (Arduino native USB)
```

**Grant permission:**
```bash
sudo usermod -aG dialout $USER
# Log out and back in for this to take effect
```

**`edge-dev.toml` configuration:**

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

**What happens:** When the AI detects `dry_soil` in `agricultural` domain, the action becomes `"increase_flow"`. The `SerialActuator` opens `/dev/ttyUSB0` and sends `INCREASE_FLOW:auto:0.85\n` to the Arduino, which opens the irrigation valve.

---

### 5.3 MQTT Broker — Smart Devices & IoT Peripherals

**Use case:** Control any MQTT-capable device — smart relays, ESPHome nodes, Zigbee bridges (via Mosquitto), industrial PLCs with MQTT adapters, or Home Assistant automations.

**Install and start Mosquitto (MQTT broker):**
```bash
sudo apt install -y mosquitto mosquitto-clients
sudo systemctl enable mosquitto
sudo systemctl start mosquitto

# Verify it's running:
sudo systemctl status mosquitto
```

**Test the broker:**
```bash
# Terminal 1 — subscribe
mosquitto_sub -h localhost -t "caesar/actions" -v

# Terminal 2 — publish a test
mosquitto_pub -h localhost -t "caesar/actions" -m '{"action":"alert","target":"auto"}'
```

**Wire an ESPHome device to subscribe to Caesar commands:**

```yaml
# ESPHome config snippet
mqtt:
  broker: 192.168.1.100   # IP of your Linux MQTT broker
  on_message:
    - topic: caesar/actions
      then:
        - lambda: |-
            auto doc = cJSON_Parse(x.c_str());
            std::string action = cJSON_GetObjectItem(doc, "action")->valuestring;
            if (action == "lock_perimeter") {
              id(gate_relay).turn_on();
            } else if (action == "alert") {
              id(siren_relay).turn_on();
              delay(5000);
              id(siren_relay).turn_off();
            }
```

**`edge-dev.toml` configuration:**

```toml
[[actuators]]
id            = "mqtt_command_bus"
actuator_type = "mqtt"
capabilities  = ["alert", "lock_perimeter", "maintenance_alert", "shutdown_zone"]
mqtt_topic    = "caesar/actions"
mqtt_broker   = "localhost:1883"

# For a remote broker (e.g., a central MQTT server):
# mqtt_broker = "192.168.1.100:1883"
```

**JSON payload sent to MQTT on each dispatch:**
```json
{
  "action": "lock_perimeter",
  "target": "auto",
  "intensity": 0.94,
  "confidence": 0.91,
  "domain": "tactical",
  "rationale": "[caesar.tactical] AI detected 'armed_intruder' via onnx over 8 snapshots."
}
```

Any device subscribed to `caesar/actions` can parse this JSON and respond accordingly.

---

## 6. Connecting a Camera

### USB Webcam

Plug in the webcam and check it's available:
```bash
ls /dev/video*          # Should see /dev/video0
v4l2-ctl --list-devices # Full device list
```

Set in `edge-dev.toml`:
```toml
[sentinel]
enabled   = true
device_id = 0    # Corresponds to /dev/video0
```

### Raspberry Pi CSI Camera

Enable in raspi-config:
```bash
sudo raspi-config
# → Interface Options → Camera → Enable
sudo reboot
```

Then test:
```bash
raspistill -o test.jpg
```

The CSI camera on RPi appears as `/dev/video0` under the libcamera stack. Set `device_id = 0`.

---

## 7. Launching the Dashboard Console

```bash
pip3 install fastapi uvicorn sse-starlette httpx
cd project-caesar
python3 services/caesar_console/server.py
```

Access the dashboard at: `http://<linux-machine-ip>:8090`

The **PIP (Picture-in-Picture)** live camera feed in the bottom-left of the map automatically switches to whichever node is reporting the most recent `high-interest` threat. Clicking any node marker on the map shows a **[ FOCUS LIVE FEED ]** button to manually pin the PIP to that node for 30 seconds.

---

## 8. Running as Systemd Services

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
User=root
WorkingDirectory=/opt/project-caesar
ExecStart=/opt/project-caesar/target/release/caesar-hub serve --config configs/hub-dev.toml
Restart=always
RestartSec=3

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
User=root
WorkingDirectory=/opt/project-caesar
ExecStart=/opt/project-caesar/target/release/uriel-edge-node --config configs/edge-dev.toml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable caesar-hub uriel-edge
sudo systemctl start caesar-hub uriel-edge
sudo systemctl status caesar-hub uriel-edge
```

---

## 9. How AI Detections Trigger Physical Actions

This is the complete pipeline from detection to device action:

```
Camera Frame (24fps)
    │
    ▼  OpenCV MOG2 Background Subtraction
Anomaly Score > threshold?  (e.g., 0.30)
    │  YES — 6 consecutive frames
    ▼
TemporalAccumulator (ring buffer, 8 snapshots)
    │  Mean confidence ≥ 0.87? OR burst score > 0.70?
    ▼
AI Inference Cascade:
    1. YOLO-World ONNX (zero-shot object detection)
    2. Ollama LLaVA VLM (visual reasoning fallback)
    3. Heuristic (domain-pattern fallback)
    │
    ▼  Class label produced, e.g. "armed_intruder"
infer_action_from_class("armed_intruder", "tactical")
    │  → "lock_perimeter"
    ▼
ActuatorBus::dispatch("lock_perimeter")
    │
    ├─▶  GpioActuator  → writes "1" to /sys/class/gpio/gpio17/value  (relay ON)
    ├─▶  MqttActuator  → publishes JSON to caesar/actions
    ├─▶  SerialActuator → sends "LOCK_PERIMETER:auto:0.94\n" over UART
    └─▶  LogActuator   → always writes structured audit log

Dashboard PIP Camera  → automatically switches to the alerting node's live MJPEG stream
```

### Domain → Action Mapping Reference

| Domain | AI Detects | Action Dispatched |
|---|---|---|
| `tactical` | `armed_intruder`, `drone`, `uav` | `lock_perimeter` |
| `tactical` | anything else | `alert` |
| `agricultural` | `dry_soil`, `crop_wilt`, `plant_stress` | `increase_flow` |
| `agricultural` | `flood`, `saturated`, `overwater` | `decrease_flow` |
| `agricultural` | `pest`, `disease` | `alert` |
| `industrial` | `overheat`, `fire`, `smoke` | `maintenance_alert` |
| `industrial` | `leak`, `spill`, `rupture` | `shutdown_zone` |
| `general` | anything | `alert` |

To add new action mappings, edit `infer_action_from_class()` in `crates/uriel-edge-node/src/actuator.rs` and add the corresponding capability to the relevant actuator entry in `edge-dev.toml`.

## 10. Full-Featured Deployment on Raspberry Pi 3 (Hybrid Distributed Architecture)

Project Caesar runs at **100% functionality** on a Raspberry Pi 3 (1GB RAM) using a
**Remote Compute Offloading** strategy. Instead of dropping to a degraded heuristic-only
mode, the Pi 3 runs all physical sensor and actuation workloads while the hub PC handles
all AI-heavy computation. The result is routed back to the Pi 3 over the local network
in under a second, seamlessly feeding into the EKF fusion engine and actuator bus.

---

### 10.1 Architecture Overview

```
┌─────────────────────── Raspberry Pi 3 ──────────────────────────┐
│  rpicam-jpeg (CSI camera) → JPEG frames                         │
│  thermal_adapter.py (I2C MLX90640) → temperature grid           │
│  radar_adapter.py (UART LD2450) → point cloud                   │
│  EKF Sensor FusionEngine                                         │
│  GPIO relay / UART serial / MQTT / Webhook Actuators            │
│  Passive Recon (PCAP, SDR, eBPF, ONVIF, USB)                   │
│  Ed25519 signing → TCP uplink to hub                            │
│                                                                  │
│  [inference] mode = "remote_http"                               │
│    Optical JPEG ─── POST /infer ──────────────────────────────► │
│               ◄─── Observation JSON (class, confidence, pos) ── │
└─────────────────────────────────────────────────────────────────┘
          │  TCP port 7878 — signed FusedTrack envelopes
          ▼
┌────────────────────── Hub PC (10.163.194.96) ────────────────────┐
│  caesar-hub          (port 7878) — receives + journals tracks   │
│  remote_infer_server (port 9090) — full Ollama VLM vision       │
│  caesar_console      (port 8090) — tactical dashboard           │
│  Ollama (llava)      (port 11434) — thermal/radar text prompts  │
└─────────────────────────────────────────────────────────────────┘
```

> **Offline resilience**: If the hub PC becomes unreachable (network outage, power loss),
> the Pi 3 automatically falls back to its **built-in heuristic engine** and continues
> capturing, sensing, fusing, and actuating hardware autonomously. Telemetry resumes
> streaming to the hub the moment the connection is restored.

---

### 10.2 Feature Retention Table

| Feature | Pi 5 Full Mode | Pi 3 Hybrid (this section) |
|---|---|---|
| Camera capture (CSI / USB) | ✅ | ✅ Identical |
| Thermal I2C sensor (MLX90640) | ✅ | ✅ Identical |
| Radar UART sensor (LD2450) | ✅ | ✅ Identical |
| YOLO-World optical classification | ✅ On Pi | ✅ On hub PC (result sent back) |
| Ollama VLM visual reasoning | ✅ On Pi | ✅ On hub PC (result sent back) |
| Ollama text thermal/radar classify | ✅ On Pi | ✅ Direct to hub port 11434 |
| Heuristic fallback (offline) | ✅ | ✅ Auto-triggers if hub unreachable |
| EKF Sensor Fusion | ✅ | ✅ Identical |
| GPIO actuator (relay) | ✅ | ✅ Identical |
| UART serial actuator (Arduino) | ✅ | ✅ Identical |
| MQTT actuator (IoT) | ✅ | ✅ Identical |
| HTTP Webhook actuator | ✅ | ✅ Identical |
| Passive Recon (PCAP/SDR/eBPF) | ✅ | ✅ Identical |
| Ed25519 envelope signing | ✅ | ✅ Identical |
| Dashboard Console | ✅ | ✅ Identical (runs on hub PC) |
| Dashboard Camera PIP stream | ✅ | ✅ Identical |
| Mesh Orchestrator | ✅ | ✅ Identical (runs on hub PC) |
| OpenCV Sentinel | ✅ On Pi | ❌ Disabled on Pi 3 (standard optical worker used instead) |

**Pi 3 RAM footprint: < 50 MB** (vs > 1 GB for ort_native mode).

---

### 10.3 Configure the Pi 3

Edit `configs/edge-pi3.toml` and update **both** occurrences of the hub IP:

```toml
[uplink]
tcp_addr = "10.163.194.96:7878"    # ← Your hub PC's local IP

[inference]
mode            = "remote_http"
ollama_endpoint = "http://10.163.194.96:9090"   # ← Same hub PC IP
```

Find your hub PC's IP on **Windows**: run `ipconfig` → look for "IPv4 Address" under
your Wi-Fi or Ethernet adapter. On **Linux**: run `ip address`.

---

### 10.4 Launch Sequence

#### Step 1 — Bootstrap the hub PC (run once)

```bash
bash scripts/bootstrap_hub.sh /opt/uriel-caesar
```

This installs Ollama, pulls the `llava` vision model, and sets up the Python environment.

#### Step 2 — Start hub services (run on your PC)

Open **three terminal windows**:

```bash
# Terminal 1 — Caesar Hub (receives signed telemetry from Pi 3)
cargo run -p caesar-hub -- --config configs/hub-dev.toml serve

# Terminal 2 — Remote Inference Server (runs Ollama VLM for the Pi 3)
python services/remote_infer_server.py --port 9090

# Terminal 3 — Dashboard Console
python services/caesar_console/server.py --host 0.0.0.0 --port 8090
```

Open `http://localhost:8090` in your browser to load the tactical dashboard.

#### Step 3 — Bootstrap the Raspberry Pi 3 (run once on Pi)

```bash
bash scripts/bootstrap_edge_pi.sh /opt/uriel-caesar
```

#### Step 4 — Launch the edge node on the Pi 3

```bash
# Build (no ONNX models needed — compiles much faster)
cargo build --release -p uriel-edge-node

# Launch with the Pi 3 profile
./target/release/uriel-edge-node --config configs/edge-pi3.toml
```

You will see output like:

```
[caesar] Booting Uriel edge node 'tower-bwari-pi3' | site='Bwari' | domain='tactical'
[caesar.sentinel] Sentinel mode inactive — standard optical worker spawned.
[edge.recon] Starting passive reconnaissance engine...
[caesar.tactical][remote_http] Inference OK — class='clear' conf=0.84 stage=ollama
[caesar.tactical][actuator.system_log] ACTION=alert TARGET=auto ...
```

---

### 10.5 Verify the Remote Inference Server

You can test the server independently from any machine:

```bash
# Health check
curl http://10.163.194.96:9090/health

# Manual inference test (send a blank JPEG)
python3 - <<'EOF'
import base64, json, urllib.request
# Minimal valid JPEG bytes
jpeg = bytes([0xFF,0xD8,0xFF,0xE0,0x00,0x10,0x4A,0x46,0x49,0x46,0x00,0x01,
              0x01,0x00,0x00,0x01,0x00,0x01,0x00,0x00,0xFF,0xD9])
body = json.dumps({"jpeg_b64": base64.b64encode(jpeg).decode(),
                   "domain": "tactical", "sequence": 1,
                   "node_id": "test-pi3", "timestamp_ms": 1000}).encode()
req = urllib.request.Request("http://10.163.194.96:9090/infer",
      data=body, headers={"Content-Type":"application/json"}, method="POST")
print(json.loads(urllib.request.urlopen(req).read()))
EOF
```

Expected response:
```json
{"track_hint": "ollama-track-1", "confidence": 0.84, "class_label": "clear", ...}
```
