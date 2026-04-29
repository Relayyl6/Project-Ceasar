# Project Caesar — Distributed Mesh Intelligence & Uriel Edge OS

Project Caesar is a fully autonomous, distributed edge-intelligence platform designed for live physical deployment across the Bwari zone, FCT. It integrates DeepMind-tier AI architectures (YOLO-World, Gemini-ER, pxADMM, AlphaStar, Thermal LSTM), hardware-native sensor fusion, federated learning, and a live command dashboard — all running on ARM64 Linux edge nodes.

---

## Architecture Overview

```
 ┌──────────────────────────────────────────────────────────────────┐
 │  HARDWARE LAYER (Raspberry Pi 5 / Jetson / STM32)                │
 │  CSI Camera → Radar (UART) → Thermal (I2C) → SDR (USB)           │
 └────────────────────┬─────────────────────────────────────────────┘
                      │  SensorBus (broadcast channels)
 ┌────────────────────▼─────────────────────────────────────────────┐
 │  URIEL EDGE NODE  (Rust / uriel-edge-node)                       │
 │  ONNX Inference → EKF Fusion → Signed Envelope → Uplink          │
 │  Principal: ort_native | Fallback: Ollama VLM | Final: heuristic │
 └────────────────────┬─────────────────────────────────────────────┘
                      │  TCP JSONL / LoRa RFD900x
 ┌────────────────────▼─────────────────────────────────────────────┐
 │  CAESAR HUB  (Rust / caesar-hub)                                 │
 │  ed25519 Verify → Persist → Journal / High-Interest JSONL        │
 └────────────────────┬─────────────────────────────────────────────┘
                      │  JSON files (output/caesar/)
 ┌────────────────────▼─────────────────────────────────────────────┐
 │  CAESAR CONSOLE  (Python / server.py)                            │
 │  REST API + SSE /api/live-events + MJPEG /api/camera-stream      │
 └────────────────────┬─────────────────────────────────────────────┘
                      │  EventSource + fetch
 ┌────────────────────▼─────────────────────────────────────────────┐
 │  DASHBOARD  (index.html / app.js / ceasar-api.js)                │
 │  Live map · Track log · YOLO feed · Anomaly stream · Camera      │
 └──────────────────────────────────────────────────────────────────┘
```

---

## Hardware Required & How to Connect

### 1. Primary Compute Node — Raspberry Pi 5 (8GB) or NVIDIA Jetson Orin Nano
**Role:** Runs `uriel-edge-node` binary. All sensors attach here.
- Flash Ubuntu 24.04 ARM64 Server. Enable SSH.
- Install Rust toolchain: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Copy the compiled binary and `models/` directory to the node.

### 2. CSI Camera — Raspberry Pi Camera Module v3 (or compatible IMX477)
**Connection:** 15-pin CSI-2 ribbon cable → CSI-2 port on Pi 5 (labelled CAM0 or CAM1)
**Activation:**
```bash
# On the Pi:
sudo raspi-config   # → Interface Options → Camera → Enable
# Verify:
libcamera-still -o test.jpg
```
**What the software does:** `camera.rs` calls `libcamera` via subprocess to capture JPEG frames. Frames are broadcast on the `SensorBus` optical channel → processed by YOLO-World ONNX pipeline at ~12 fps.

**If connecting a USB webcam instead:**
```bash
ls /dev/video*   # should show /dev/video0
```
The server's `_init_camera()` opens `cv2.VideoCapture(0)` automatically. No config needed.

### 3. Radar Module — LD2450 24GHz mmWave (UART)
**Connection:** 
```
LD2450 TX  →  Pi GPIO 15 (UART RX, pin 15)
LD2450 RX  →  Pi GPIO 14 (UART TX, pin 14)
LD2450 GND →  Pi GPIO GND (pin 6 or 9)
LD2450 VCC →  Pi 3.3V or 5V (check module datasheet)
```
**Activation:**
```bash
sudo raspi-config   # → Interface Options → Serial Port
# Disable login shell over serial, ENABLE serial hardware
# Port will appear as /dev/ttyAMA0
```
**What the software does:** `sensors.rs` auto-discovers UART ports via `serialport::available_ports()`. When the LD2450 is connected, it reads binary sweep frames and broadcasts them on the `RadarSweep` channel. The `radar_infer()` pipeline converts range+azimuth into `Observation` with `Modality::Radar`.

### 4. Thermal Camera — MLX90640 (I2C)
**Connection:**
```
MLX90640 SDA  →  Pi GPIO 2  (I2C1 SDA, pin 3)
MLX90640 SCL  →  Pi GPIO 3  (I2C1 SCL, pin 5)
MLX90640 GND  →  Pi GND     (pin 6)
MLX90640 VCC  →  Pi 3.3V    (pin 1)
```
**Activation:**
```bash
sudo raspi-config   # → Interface Options → I2C → Enable
i2cdetect -y 1     # should show device at 0x33
```
**What the software does:** `hal.rs`'s `ThermalI2CSensor` uses `rppal::i2c` to read 32×24 temperature matrices at 2Hz. Frames are broadcast on the `ThermalFrame` channel → processed by the `seq2seq_thermal_lstm.onnx` pipeline.

### 5. SDR (Software-Defined Radio) — RTL-SDR v3 (USB)
**Connection:** Insert RTL-SDR dongle into any Pi USB port. Attach the included magnetic-mount antenna, oriented vertically.
**Activation:**
```bash
sudo apt install rtl-sdr
rtl_test   # confirms device found
```
**What the software does:** `recon.rs` calls `SoapySDR` to scan the 433MHz, 868MHz, and 915MHz LoRa bands. Detected RF anomalies (unexpected transmissions, signal spikes) are converted to `Observation` with `Modality::Radar` and uplinked.

### 6. Long-Range Uplink — RFD900x Modem (UART / USB-UART)
**Connection (USB-to-UART adapter):**
```
RFD900x TX  →  USB-UART RX
RFD900x RX  →  USB-UART TX
RFD900x GND →  USB-UART GND
Power RFD900x via dedicated 5V supply (draws up to 1.5A peak)
```
**Activation:** 
- The adapter appears as `/dev/ttyUSB0` or `/dev/ttyACM0`.
- `uplink.rs` automatically scans all available serial ports and writes signed JSONL envelopes at 57600 baud.
- Air data rate must be configured ≥ 64 kbps via RFD Tools (Windows) or AT commands before deployment.

### 7. Hub Machine — Any Linux server or second Pi
**Role:** Runs `caesar-hub` binary and `server.py`. Reachable by all edge nodes over LAN or LoRa mesh.
```bash
cargo run --bin caesar-hub -- --config configs/hub.toml
python services/caesar_console/server.py   # auto-installs opencv, pillow
```
Open browser: `http://<hub-ip>:8090`

---

## First-Time Setup (Any Machine)

### Step 1 — Generate ONNX Models
```powershell
# Windows dev machine:
.\venv\Scripts\activate
python models/setup_advanced_models.py
# Models output to models/ — copy this directory to edge nodes
```

### Step 2 — Build Edge Node for ARM64
```bash
# Install cross-compiler on your dev machine:
rustup target add aarch64-unknown-linux-gnu
cargo build --bin uriel-edge-node --target aarch64-unknown-linux-gnu --release

# Copy to edge node:
scp target/aarch64-unknown-linux-gnu/release/uriel-edge-node pi@<node-ip>:~/
scp -r models/ pi@<node-ip>:~/models/
scp configs/edge-dev.toml pi@<node-ip>:~/config.toml
```

### Step 3 — Run Edge Node (on Pi)
```bash
./uriel-edge-node --config config.toml
```
The node will:
1. Auto-discover all connected UART devices → try LD2450 radar and RFD900x
2. Auto-discover I2C bus → try MLX90640 thermal
3. Open `/dev/video0` CSI camera or fall back to synthetic frames
4. Load ONNX models from `models/`
5. Start the ONNX→Ollama→Heuristic inference cascade
6. Sign and uplink fused tracks every `fusion_window_ms` milliseconds

### Step 4 — Run Hub & Dashboard
```bash
cargo run --bin caesar-hub -- --config configs/hub.toml
python services/caesar_console/server.py   # installs deps automatically
```

### Step 5 (Optional) — Ollama Fallback VLM
```bash
ollama run llava   # starts background vision model on port 11434
```
The edge node will fall back to this automatically if ONNX inference fails.

---

## Dashboard — What Each Panel Shows

| Panel | Data Source | Live? |
|---|---|---|
| Active Mesh Tracks | `/api/stats` + SSE push | ✅ |
| Regional Throughput | `/api/stats` + SSE push | ✅ |
| Anomaly Probability | `/api/stats` + SSE push | ✅ |
| FedPDM Alignment | `/api/learning-plan` | ✅ |
| Map + Node Markers | `/api/latest` + `/api/node-registry` | ✅ |
| pxADMM Heatmap | Computed from active tracks | ✅ |
| YOLO Feed Log | Appended per detection cycle | ✅ |
| Confidence Bars | Computed from threat ratios | ✅ |
| Learning Fabric | `/api/learning-plan` | ✅ |
| Track Log Table | `/api/latest` rolling window | ✅ |
| Live Anomaly Stream | high-interest events | ✅ |
| Governance Audit | `/api/governance-audit` | ✅ |
| Camera Feed | `/api/camera-stream` MJPEG | ✅ |
| Footer Ticker | Latest track summary | ✅ |

Nothing in the dashboard is static. Every element updates on the SSE push cycle (every 2 seconds from the server) or the simulation advance cycle (every 1.2 seconds when offline).
