# Project Caesar — Distributed Edge Intelligence Platform

**Uriel Edge OS** — autonomous, multi-modal, AI-gated sensing with real hardware fallback paths, ed25519-signed telemetry, and an always-on OpenCV sentinel pipeline.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  HARDWARE LAYER  (Raspberry Pi 5 / Jetson Orin / STM32 co-proc)     │
│  CSI Camera · LD2450 Radar (UART) · MLX90640 Thermal (I2C)          │
│  RTL-SDR (USB) · MEMS Mic (I2S/ALSA) · RFD900x LoRa Modem (UART)    │
└──────────────────────────┬──────────────────────────────────────────┘
                           │  SensorBus (Tokio broadcast channels)
┌──────────────────────────▼─────────────────────────────────────────────┐
│  URIEL EDGE NODE  (Rust · crates/uriel-edge-node)                      │
│                                                                        │
│  ┌─────────────────── SENTINEL MODE ─────────────────────────────┐     │
│  │  OpenCV VideoCapture @ 24 fps                                 │     │
│  │  BackgroundSubtractorMOG2  →  anomaly_score                   │     │
│  │  6-frame rolling gate  →  TemporalAccumulator (ring buffer)   │     │
│  │  score > 0.30: snapshot  │  score > 0.70: burst AI now        │     │
│  │  8 snapshots + conf ≥ 0.87  →  AI + ActuatorBus dispatch      │     │
│  │  Always: live JPEG → SentinelFeedEvent → dashboard channel    │     │
│  │  Optional: MJPEG HTTP server on configured port               │     │
│  └───────────────────────────────────────────────────────────────┘     │
│                                                                        │
│  ┌──────────────── STANDARD MODE (sentinel.enabled=false) ───────┐     │
│  │  optical / thermal / radar SensorBus → inference workers      │     │
│  └───────────────────────────────────────────────────────────────┘     │
│                                                                        │
│  AI Cascade: ONNX (ORT native) → Ollama VLM → Heuristic                │
│  Pipelines:  YOLO-World · Gemini-ER · Seq2Seq LSTM · pxADMM            │
│  Fusion:     Extended Kalman Filter · Z-score acoustic tracking        │
│  Security:   ed25519 sign · Noise_XX session · blake3 digests          │
│  Recon:      eBPF kprobes · USB fingerprint · ONVIF scan · SDR EW      │
│  Actuators:  LogActuator · SerialActuator · GpioActuator · MqttActuator│
└──────────────────────────┬─────────────────────────────────────────────┘
                           │  TCP JSONL / LoRa RFD900x / File / Gossipsub
┌──────────────────────────▼─────────────────────────────────────────────┐
│  CAESAR HUB  (Rust · crates/caesar-hub)                                │
│  TcpListener → ed25519 verify → HubStore persist                       │
│  high-interest JSONL stream · trusted-key allowlist                    │
└──────────────────────────┬─────────────────────────────────────────────┘
                           │  output/caesar/ JSONL files
┌──────────────────────────▼─────────────────────────────────────────────┐
│  CAESAR CONSOLE  (Python · services/caesar_console/server.py)          │
│  REST API · SSE /api/live-events · MJPEG /api/camera-stream            │
└──────────────────────────┬─────────────────────────────────────────────┘
                           │  EventSource + fetch
┌──────────────────────────▼─────────────────────────────────────────────┐
│  DASHBOARD  (index.html · app.js · ceasar-api.js)                      │
│  Live map · Track log · YOLO feed · Anomaly stream · Camera            │
└────────────────────────────────────────────────────────────────────────┘
```

---

## Implemented Features (Verified)

### Core Infrastructure (`crates/uriel-caesar-core`)

| Feature | File | Status |
|---|---|---|
| TOML config loader (`read_toml<T>`) | `src/io.rs` | ✅ |
| Unix timestamp helper (`unix_time_ms`) | `src/io.rs` | ✅ |
| JSON-line serialiser (`to_json_line`) | `src/io.rs` | ✅ |
| ed25519 envelope signing + verification | `src/crypto.rs` | ✅ |
| Noise_XX_25519_ChaChaPoly_BLAKE2s session | `src/crypto.rs` | ✅ |
| Protocol types: `Observation`, `FusedTrack`, `SignedEnvelope` | `src/protocol.rs` | ✅ |
| `Modality` enum: Optical · Thermal · Radar · Manual · **Sentinel** | `src/protocol.rs` | ✅ |
| `SentinelAlert` dashboard telemetry struct | `src/protocol.rs` | ✅ |
| HAL sensor traits + simulated sensors | `src/hal.rs` | ✅ |
| Physical radar (TI IWR6843 UART TLV) | `src/hal.rs` | ✅ |
| Physical thermal (FLIR Boson+ I2C, Linux) | `src/hal.rs` | ✅ |
| Physical acoustic (I2S MEMS via cpal, Linux) | `src/hal.rs` | ✅ |

---

### Edge Node (`crates/uriel-edge-node`)

#### Configuration (`src/config.rs`)

| Feature | Status |
|---|---|
| `EdgeConfig` with all sensor + uplink sections | ✅ |
| `SentinelConfig` — device_id, fps, thresholds, autonomous_confidence, mjpeg_port | ✅ |
| `ActuatorConfig` — type, capabilities, gpio_pin, serial_port, baud_rate, mqtt_topic, **mqtt_broker** | ✅ |
| Serde defaults for all sentinel fields | ✅ |
| Deployment domain field (`agricultural` / `industrial` / `tactical` / `general`) | ✅ |

#### SensorBus (`src/sensors.rs`)

| Feature | Status |
|---|---|
| Tokio broadcast channels for Optical / Thermal / Radar | ✅ |
| Synthetic frame generators (no hardware required) | ✅ |
| `file_json` mode — replay from JSON files | ✅ |
| `command_json` mode — delegate to external subprocess | ✅ |
| Auto-spawning source tasks from config | ✅ |

#### Camera (`src/camera.rs`)

| Feature | Status |
|---|---|
| `libcamera-still` CSI capture (RPi) | ✅ |
| V4L2/ffmpeg MJPEG capture (USB webcam) | ✅ |
| Arducam CSI + CSI port selection | ✅ |
| Synthetic frame generator | ✅ |
| Temp-file bridge for subprocess inference | ✅ |

#### OpenCV Sentinel Pipeline (`src/sentinel.rs`)

| Feature | Status |
|---|---|
| `VideoCapture` @ configurable fps | ✅ |
| `BackgroundSubtractorMOG2` (history=500, var=16) | ✅ |
| Top-4 contour extraction by area | ✅ |
| Anomaly score = foreground_area / frame_area | ✅ |
| Amber bounding-box annotations on live frame | ✅ |
| **6-frame rolling confirmation gate** | ✅ |
| `TemporalAccumulator` ring buffer (2× cap, slide window) | ✅ |
| `ChangePortrait` 4-quadrant 2×2 composite JPEG | ✅ |
| `SentinelFeedEvent` broadcast channel (LiveFrame / PortraitReady / ActuatorDispatched) | ✅ |
| Burst escalation path (score > 0.70 → skip accumulation) | ✅ |
| Autonomous dispatch (8 snapshots + mean_conf ≥ 0.87) | ✅ |
| `SentinelAlert` JSON emission for hub ingestion | ✅ |
| Inference stage tagging (`onnx` / `ollama` / `heuristic`) | ✅ |
| **MJPEG HTTP server** (`multipart/x-mixed-replace`) on `sentinel.mjpeg_port` | ✅ |
| Dedicated OS thread (keeps Tokio executor free) | ✅ |
| Unit tests: accumulator logic, capacity cap, sliding window | ✅ |

#### Inference Pipeline (`src/inference.rs`)

| Feature | Status |
|---|---|
| `SentinelContext` struct | ✅ |
| `optical_infer_with_sentinel()` — gate by burst_threshold | ✅ |
| `OrtYoloPipeline` — NCHW ONNX inference, CLIP vocab, NMS | ✅ |
| `GeminiRoboticsERPipeline` — embodied reasoning ONNX | ✅ |
| Ollama VLM vision fallback — domain-aware prompt | ✅ |
| Heuristic optical fallback | ✅ |
| `seq2seq_thermal_lstm.onnx` thermal pipeline | ✅ |
| Ollama thermal text-prompt fallback | ✅ |
| `pxADMM` radar anomaly ONNX pipeline | ✅ |
| Ollama radar text-prompt fallback | ✅ |
| Heuristic final fallback (all modalities) | ✅ |
| `command_json` subprocess inference mode | ✅ |
| Domain-aware Ollama prompts (agricultural / industrial / tactical / general) | ✅ |
| blake3 evidence digests on all observations | ✅ |

#### Fusion Engine (`src/fusion.rs`)

| Feature | Status |
|---|---|
| Tokio-select observation bucket aggregation | ✅ |
| Extended Kalman Filter (predict + confidence-weighted update) | ✅ |
| Z-score acoustic/vibration anomaly tracking (100-sample rolling) | ✅ |
| SCLE panic isolation boundary | ✅ |
| Threat-level classification (`high-interest` / `monitor`) | ✅ |
| `FusedTrack` → `SignedEnvelope` → Uplink | ✅ |

#### Actuator Bus (`src/actuator.rs`)

| Actuator | Behaviour | Status |
|---|---|---|
| `LogActuator` | Domain-prefixed stdout + audit trail; always registered | ✅ |
| `SerialActuator` | `ACTION:TARGET:INTENSITY\n` over UART (serialport crate) | ✅ |
| `GpioActuator` | `/sys/class/gpio/gpioN/value` toggle (Linux/cfg-gated) | ✅ |
| **`MqttActuator`** | Raw MQTT 3.1.1 CONNECT + PUBLISH over TCP (no extra dep) | ✅ |
| `ActuatorBus::from_config()` | Builds all four types from TOML `[[actuators]]` | ✅ |
| `dispatch()` — routes to matching capabilities | Always sends to `system_log` for audit | ✅ |
| `infer_action_from_class()` | Domain-aware label→action mapping | ✅ |
| Unit tests: action mapping, bus dispatch | ✅ |

#### Uplink (`src/uplink.rs`)

| Mode | Status |
|---|---|
| `stdout` | ✅ |
| `file` (append JSONL) | ✅ |
| `tcp_jsonl` (auto-reconnect BufWriter) | ✅ |
| `gossipsub` (libp2p stub + RFD900x UART auto-scan) | ✅ |

#### Recon Worker (`src/recon.rs`)

| Feature | Status |
|---|---|
| eBPF kprobes via Aya (`sys_enter`, `SSL_read` uprobe) | ✅ (Linux) |
| USB device fingerprinting via nusb | ✅ |
| ONVIF/RTSP subnet scan (ports 554) | ✅ |
| 802.11 monitor-mode PCAP probe ingestion | ✅ (Linux) |
| SoapySDR / RTL-SDR 2.4GHz RF waterfall scan | ✅ (Linux) |
| Simulation fallback on non-Linux | ✅ |

---

### Caesar Hub (`crates/caesar-hub`)

| Feature | Status |
|---|---|
| TCP listener + concurrent connection handling | ✅ |
| ed25519 signature verification on every envelope | ✅ |
| Trusted public-key allowlist | ✅ |
| `HubStore` — persist to JSONL journal | ✅ |
| High-interest track stream (separate file) | ✅ |
| `cargo run --bin caesar-hub -- serve` | ✅ |
| `cargo run --bin caesar-hub -- latest` | ✅ |

---

### Workspace (`Cargo.toml`)

| Dependency | Purpose | Status |
|---|---|---|
| `anyhow`, `serde`, `serde_json`, `toml` | Error handling, serialisation | ✅ |
| `tokio` (full features) | Async runtime | ✅ |
| `clap` (derive) | CLI argument parsing | ✅ |
| `ed25519-dalek` | Envelope signing | ✅ |
| `blake3` | Evidence digests | ✅ |
| `base64` | Portrait JPEG encoding | ✅ |
| `ort 2.0.0-rc.2` | ONNX Runtime inference | ✅ |
| `ndarray` | Tensor construction | ✅ |
| `opencv 0.93` (videoio, video, imgproc, imgcodecs) | Sentinel pipeline | ✅ |
| `libp2p` (gossipsub, noise, tcp, yamux) | Mesh uplink | ✅ |
| `snow` | Noise protocol sessions | ✅ |

---

## Configuration Reference (`configs/edge-dev.toml`)

```toml
# Top-level node identity
node_id          = "tower-bwari-alpha"
publish_topic    = "caesar_tactical_intel"
loop_count       = 0               # 0 = run forever
fusion_window_ms = 600
threat_threshold = 0.72
ed25519_seed_hex = "<32-byte hex seed>"
domain           = "tactical"      # agricultural | industrial | tactical | general
recon_enabled    = true

[location]
site      = "Bwari"
latitude  = 9.2797
longitude = 7.3781

[uplink]
mode     = "tcp_jsonl"             # stdout | file | tcp_jsonl | gossipsub
tcp_addr = "127.0.0.1:7878"

[optical]
enabled           = true
mode              = "synthetic"    # synthetic | rpi_csi | arducam_v4l2_ffmpeg | command_json | file_json
camera_id         = "optical-front"
width             = 1280
height            = 720
frame_interval_ms = 500

[thermal]
enabled           = true
mode              = "synthetic"    # synthetic | command_json | file_json
camera_id         = "thermal-front"
width             = 640
height            = 512
frame_interval_ms = 350

[radar]
enabled           = true
mode              = "synthetic"    # synthetic | command_json | file_json
radar_id          = "mmwave-front"
point_count       = 48
frame_interval_ms = 325

[inference]
mode             = "ort_native"    # ort_native | heuristic | command_json
model_yolo_world = "models/yolo_world_v2_s.onnx"
model_gemini_er  = "models/gemini_robotics_er_1_6.onnx"
model_seq2seq    = "models/seq2seq_thermal_lstm.onnx"
model_pxadmm     = "models/pxadmm_anomaly.onnx"
ollama_endpoint  = "http://localhost:11434"
ollama_model     = "llava"
vocabulary       = ["human intruder", "armed person", ...]

[sentinel]
enabled               = false      # flip to true with a real camera attached
device_id             = 0          # OpenCV VideoCapture index
fps                   = 24
motion_area_threshold = 500.0      # minimum contour area px²
anomaly_threshold     = 0.30       # start accumulating above this score
burst_threshold       = 0.70       # skip accumulation, escalate AI immediately
min_snapshots         = 8          # frames needed before autonomous dispatch
autonomous_confidence = 0.87       # mean confidence gate for actuator fire
# snapshot_dir        = "data/snapshots"
# mjpeg_port          = 8090       # serves live MJPEG at http://<node>:8090

[[actuators]]
id            = "dashboard_notify"
actuator_type = "log"              # log | serial | gpio | mqtt
capabilities  = ["alert", "increase_flow", ...]

# [[actuators]]
# id            = "irrigation_zone_3"
# actuator_type = "serial"
# capabilities  = ["increase_flow", "decrease_flow", "stop_flow"]
# serial_port   = "/dev/ttyUSB0"
# baud_rate     = 9600

# [[actuators]]
# id            = "perimeter_relay_north"
# actuator_type = "gpio"           # Linux only
# capabilities  = ["lock_perimeter", "alert"]
# gpio_pin      = 17

# [[actuators]]
# id            = "mqtt_command_bus"
# actuator_type = "mqtt"
# capabilities  = ["alert", "lock_perimeter", "maintenance_alert"]
# mqtt_topic    = "caesar/actions"
# mqtt_broker   = "localhost:1883"
```

---

## Hardware Setup

### 1. Compute Node — Raspberry Pi 5 (8 GB) or Jetson Orin Nano

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install OpenCV system libraries (required for sentinel pipeline)
sudo apt install -y libopencv-dev pkg-config

# Install Ollama (optional VLM fallback)
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llava
```

### 2. CSI Camera (RPi Camera Module v3 / IMX477)
```bash
# Connect via 15-pin CSI-2 ribbon to CAM0/CAM1
sudo raspi-config   # → Interface Options → Camera → Enable
libcamera-still -o test.jpg   # verify
```
Set `optical.mode = "rpi_csi"` in config.

### 3. USB Webcam (OpenCV sentinel mode)
```bash
ls /dev/video*   # confirm /dev/video0
```
Set `sentinel.enabled = true`, `sentinel.device_id = 0`.

### 4. LD2450 Radar (UART)
```
LD2450 TX  → Pi GPIO 15 (UART RX)
LD2450 RX  → Pi GPIO 14 (UART TX)
LD2450 GND → Pi GND
LD2450 VCC → Pi 3.3V/5V
```
```bash
sudo raspi-config   # → Interface Options → Serial → disable shell, enable hardware
# Port appears as /dev/ttyAMA0
```

### 5. MLX90640 Thermal (I2C)
```
MLX90640 SDA → Pi GPIO 2  (pin 3)
MLX90640 SCL → Pi GPIO 3  (pin 5)
MLX90640 GND → Pi GND
MLX90640 VCC → Pi 3.3V
```
```bash
sudo raspi-config   # → Interface Options → I2C → Enable
i2cdetect -y 1     # should show 0x33
```

### 6. RTL-SDR (USB, Linux recon only)
```bash
sudo apt install rtl-sdr
rtl_test   # confirm device
```

### 7. RFD900x LoRa Modem (UART uplink)
Connect via USB-to-UART adapter. Set uplink `mode = "gossipsub"`. The node auto-scans all serial ports and writes at 57600 baud.

---

## First-Time Setup

### Step 1 — Generate ONNX Models
```powershell
# Windows dev machine
.venv\Scripts\activate
python models/setup_advanced_models.py
# Models written to models/ — copy directory to edge nodes
```

### Step 2 — Build for ARM64
```bash
rustup target add aarch64-unknown-linux-gnu
# Install cross-compilation linker:
sudo apt install gcc-aarch64-linux-gnu

export OPENCV_LINK_LIBS="opencv_core,opencv_imgproc,opencv_imgcodecs,opencv_videoio,opencv_video"
export OPENCV_LINK_SEARCH_PATHS="/usr/lib/aarch64-linux-gnu"
export OPENCV_INCLUDE_PATHS="/usr/include/opencv4"

cargo build --bin uriel-edge-node --target aarch64-unknown-linux-gnu --release

scp target/aarch64-unknown-linux-gnu/release/uriel-edge-node pi@<node-ip>:~/
scp -r models/ pi@<node-ip>:~/models/
scp configs/edge-dev.toml pi@<node-ip>:~/config.toml
```

### Step 3 — Run Edge Node (on Pi)
```bash
./uriel-edge-node --config config.toml
```

Boot sequence:
1. Loads TOML config, boots ed25519 signer
2. If `sentinel.enabled = true`: opens OpenCV capture, starts MOG2 loop
3. Else: spawns optical / thermal / radar sensor bus workers
4. Starts Recon worker (eBPF / USB / ONVIF / SDR)
5. Fusion engine aggregates observations → EKF → signed envelopes
6. Uplink publishes to hub

### Step 4 — Run Hub
```bash
cargo run --bin caesar-hub -- --config configs/hub-dev.toml serve
```

### Step 5 — Run Dashboard Console
```bash
python services/caesar_console/server.py
# Open http://localhost:8090
```

### Step 6 (Optional) — MJPEG Live Feed
```toml
# In edge-dev.toml [sentinel] section:
mjpeg_port = 8091
```
View at `http://<node-ip>:8091` in any browser or VLC.

---

## Inference Cascade

```
Frame arrives (from sentinel OR sensor bus)
       │
       ▼
1. ONNX (ort_native mode)
   ├── GeminiRoboticsERPipeline  (model_gemini_er set?)
   └── OrtYoloPipeline           (model_yolo_world set?)
       │
       │ ONNX failed?
       ▼
2. Ollama VLM (http://localhost:11434)
   Domain-aware single-label prompt
       │
       │ Ollama unreachable?
       ▼
3. Heuristic
   Optical: pixel-sum brightness → confidence
   Thermal: peak temperature → class label
   Radar:   mean velocity → class label
```

---

## Deployment Domains

| Domain | Sensor Focus | Actuator Actions |
|---|---|---|
| `agricultural` | Crop stress, irrigation, pest damage | `increase_flow`, `decrease_flow`, `alert` |
| `industrial` | Overheat, leaks, structural anomaly | `maintenance_alert`, `shutdown_zone` |
| `tactical` | Intruders, drones, vehicles | `lock_perimeter`, `alert` |
| `general` | Catch-all | `alert` |

Domain affects: Ollama prompt style, `infer_action_from_class()` mapping, and all log prefixes.

---

## Dashboard Panels

| Panel | Source | Live |
|---|---|---|
| Active Mesh Tracks | `/api/stats` + SSE | ✅ |
| Regional Throughput | `/api/stats` + SSE | ✅ |
| Anomaly Probability | `/api/stats` + SSE | ✅ |
| Map + Node Markers | `/api/latest` + `/api/node-registry` | ✅ |
| pxADMM Heatmap | Active track computation | ✅ |
| YOLO Feed Log | Per detection cycle | ✅ |
| Confidence Bars | Threat ratio computation | ✅ |
| Track Log Table | `/api/latest` rolling window | ✅ |
| Live Anomaly Stream | High-interest JSONL events | ✅ |
| Governance Audit | `/api/governance-audit` | ✅ |
| Camera Feed (MJPEG) | `/api/camera-stream` | ✅ |
| Sentinel Portrait | 4-quadrant change panel | ✅ |

---

## Workspace Structure

```
.
├── Cargo.toml                        # workspace root
├── configs/
│   ├── edge-dev.toml                 # edge node reference config
│   └── hub-dev.toml                  # hub reference config
├── crates/
│   ├── uriel-caesar-core/            # shared types + crypto
│   │   └── src/
│   │       ├── crypto.rs             # ed25519 + Noise_XX
│   │       ├── hal.rs                # sensor HAL traits + impls
│   │       ├── io.rs                 # TOML loader, timestamps
│   │       └── protocol.rs           # Observation, FusedTrack, SentinelAlert
│   ├── uriel-edge-node/              # edge intelligence binary
│   │   └── src/
│   │       ├── main.rs               # boot, wiring, uplink loop
│   │       ├── config.rs             # EdgeConfig + all sub-configs
│   │       ├── sentinel.rs           # OpenCV MOG2 pipeline + MJPEG server
│   │       ├── actuator.rs           # Log / Serial / GPIO / MQTT actuators
│   │       ├── inference.rs          # ONNX → Ollama → Heuristic cascade
│   │       ├── fusion.rs             # EKF + Z-score fusion engine
│   │       ├── sensors.rs            # SensorBus + source workers
│   │       ├── camera.rs             # optical frame capture
│   │       ├── recon.rs              # eBPF / USB / ONVIF / SDR recon
│   │       └── uplink.rs             # TCP / file / LoRa uplink
│   └── caesar-hub/                   # regional hub binary
│       └── src/
│           ├── main.rs
│           ├── server.rs             # TCP listener + ed25519 verify
│           ├── store.rs              # JSONL persistence + high-interest filter
│           └── config.rs
├── models/                           # ONNX model directory
│   └── setup_advanced_models.py      # model generation script
├── services/
│   ├── caesar_console/               # Python REST + SSE dashboard server
│   └── mesh_orchestrator/            # mesh coordination utilities
└── deploy/                           # deployment scripts
```

---

## Running Tests

```bash
# All unit tests
cargo test --workspace

# Specific crate tests
cargo test -p uriel-edge-node
cargo test -p uriel-caesar-core

# Tests included:
# - crypto: ed25519 sign/verify round-trip
# - sentinel: TemporalAccumulator logic, capacity cap, sliding window
# - actuator: domain action mapping, bus dispatch routing
```

---

## System Requirements

| Requirement | Minimum | Recommended |
|---|---|---|
| Rust | 1.75 | 1.80+ |
| OpenCV | 4.8 | 4.10 |
| RAM (edge node) | 2 GB | 8 GB (Pi 5) |
| RAM (hub) | 512 MB | 2 GB |
| Disk (models) | 500 MB | 2 GB |
| OS (edge) | Ubuntu 22.04 ARM64 | Ubuntu 24.04 ARM64 |
| OS (hub/dev) | Any Rust-supported | Linux / Windows |

> **OpenCV build note:** On Windows dev machines set `OPENCV_LINK_LIBS`, `OPENCV_LINK_SEARCH_PATHS`, and `OPENCV_INCLUDE_PATHS` env vars before `cargo build`. On Linux the `pkg-config` auto-detection works after `sudo apt install libopencv-dev`.

---

## Security Model

- Every `FusedTrack` is **ed25519-signed** by the edge node before transmission
- The hub **verifies** every signature; envelopes failing verification are dropped
- An optional **trusted_public_keys** allowlist in hub config provides node authentication
- Evidence integrity: every `Observation` carries a **blake3 hash** of the raw sensor bytes
- Transport encryption: **Noise_XX** session establishment available for hub↔node channels
- Autonomous actuator actions require **both** 8 confirmed snapshots **and** ≥ 0.87 mean confidence before firing — no single frame can trigger physical output

---

## License

Private — Project Caesar / Uriel Edge OS. All rights reserved.

---

## Placeholder & Simulated Data Documentation

Project Caesar includes built-in fallback mechanisms to ensure the system can be developed and demonstrated even when physical hardware is not present. These simulated data streams are explicitly logged and only activate as a last resort.

**1. Dashboard Simulation (`ceasar-api.js`)**
If the Edge nodes are not actively publishing data or the backend server goes offline, the frontend will automatically switch to a simulation loop. This keeps the maps, heatmaps, and trackers populated. A `console.warn` is explicitly emitted in the browser devtools when this occurs.

**2. Camera Fallback (`server.py`)**
If OpenCV cannot bind to a physical webcam on device 0, the Python backend (`_synthetic_jpeg`) will generate a static "NO SIGNAL - CAMERA OFFLINE" fallback image. This ensures that the frontend camera feed gracefully degrades rather than failing silently or presenting a fake, animated UI.

**3. Sensor Mocking (`hal.rs`)**
The Hardware Abstraction Layer includes explicit `Simulated*` structs (e.g., `SimulatedThermalSensor`) designed for local development. Physical implementations like `PhysicalAcousticSensor` attempt to use native OS audio APIs (`cpal`), but will safely log a warning and return default/fallback buffers if the hardware initialization fails.

**4. Inference Stubs (`inference.rs`)**
To ensure the Rust edge node compiles and runs without needing a heavy multi-gigabyte text encoder, `encode_vocabulary` attempts to call a local Ollama embedding endpoint (`nomic-embed-text`). If Ollama is unavailable, a deterministic hashing scheme is used to populate the embedding tensor, accompanied by a warning log.
