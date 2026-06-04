# Project Caesar: Comprehensive Deployment & Hardware Setup Guide

This document provides the exact, precise steps required to provision, deploy, and operationalize the complete Project Caesar intelligence architecture. It covers edge node hardware (sensors and wireless actuators), core network configuration, and dashboard command execution.

## Phase 1: Hardware Assembly & Edge Provisioning

### 1. Optical & Acoustic Sensors (Sentinel Mode)
1. **Camera Selection**: Connect a physical webcam (USB) or CSI ribbon camera (e.g., Raspberry Pi Camera Module 3 or Arducam) to the edge device.
2. **Audio Setup**: Connect an I2S MEMS microphone (like the INMP441) or a standard USB microphone.
3. **OS Configuration (Linux)**:
   - Run `ls /dev/video*` to verify the camera appears (usually `/dev/video0`). If on a Jetson or Pi 5 using `libcamera`, ensure the V4L2 compatibility layer is active.
   - Run `arecord -l` to list audio input devices. Note the card number. The `PhysicalAcousticSensor` defaults to the ALSA `hw:0,0` fallback if PipeWire isn't installed.

### 2. Physical Actuators (Relays, Taps, Gates)
Project Caesar can now receive remote commands from the dashboard.
1. **GPIO Relays (e.g., Gate Motors, Irrigation Valves)**:
   - Connect a 3.3V/5V relay module to the Raspberry Pi/Jetson GPIO pins. 
   - Connect the relay output to your irrigation solenoid valve or gate motor logic board.
   - Note the Broadcom (BCM) GPIO pin number (e.g., Pin 17).
2. **Serial / UART Controllers (Farms & Industrial)**:
   - For distant farm nodes without IP connectivity, connect an RFD900x LoRa radio to `/dev/ttyUSB0` or `/dev/ttyAMA0`.
   - The edge node will send `ACTION:TARGET:INTENSITY\n` over UART to the local micro-controller.
3. **MQTT Smart Plugs**:
   - If using off-the-shelf smart plugs (e.g., Tasmota or Zigbee2MQTT), ensure an MQTT broker is running on the local network.

---

## Phase 2: Configuration & Compilation

### 1. Edge Node Configuration (`configs/edge-dev.toml`)
Ensure your TOML file is configured for both sensing and the newly implemented bidirectional actuation:

```toml
node_id = "tower-bwari-alpha"
domain = "agricultural"
ed25519_seed_hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
publish_topic = "caesar_tactical_intel"

[location]
site = "sector_7_farm"
geo_latitude = 9.0765
geo_longitude = 7.3986

[sentinel]
enabled = true
anomaly_threshold = 0.35
burst_threshold = 0.70
autonomous_confidence = 0.85
min_snapshots = 6

[inference]
mode = "ort_native"
# The system now uses tokio::task::spawn_blocking for non-blocking execution!

[[actuators]]
id = "irrigation_valve_main"
actuator_type = "gpio"
capabilities = ["increase_flow", "decrease_flow", "stop_flow"]
gpio_pin = 17

[[actuators]]
id = "farm_local_mqtt"
actuator_type = "mqtt"
capabilities = ["activate_irrigation", "shutdown_zone"]
mqtt_broker = "127.0.0.1:1883"
mqtt_topic = "farm/valves/control"
```

### 2. Compiling the Rust Binaries
Compile the Edge Node and Hub on the target hardware (or cross-compile):
```bash
# Compile the Hub (receives intel from edge nodes)
cargo build --release -p caesar-hub

# Compile the Edge Node (runs OpenCV sentinel and AI models)
cargo build --release -p uriel-edge-node
```

> [!TIP]
> **OpenCV Dependency**: You must have `libopencv-dev` and `clang` installed on the host machine to compile the `uriel-edge-node`.

---

## Phase 3: Bringing the System Online

### 1. Start the Command Plane (MQTT Broker)
If you are using MQTT for dashboard-to-edge command routing, install and start Mosquitto on the central server (or edge node, depending on topology):
```bash
sudo apt install mosquitto mosquitto-clients
sudo systemctl start mosquitto
```

### 2. Start the Caesar Hub
The Hub ingests the cryptographically signed `FusedTrack` JSONL stream.
```bash
./target/release/caesar-hub --config configs/hub-dev.toml
```

### 3. Start the Edge Node
Deploy the edge node in the field. It will automatically initialize the camera, wait for anomalies, and subscribe to `caesar/commands/<node_id>` for bidirectional dashboard control.
```bash
./target/release/uriel-edge-node --config configs/edge-dev.toml
```

### 4. Start the Dashboard (Python API + Web)
The Python server now features SSE heartbeats (preventing proxy drops) and the new `/api/actuate` command endpoint.
```bash
pip install opencv-python pillow paho-mqtt
cd services/caesar_console
python server.py --port 8090
```

---

## Phase 4: Remote Hardware Control (Dashboard -> Edge)

With the new architecture, the `ActuatorBus` is fully bidirectional.

1. **Autonomous Operation**: If the Sentinel OpenCV pipeline detects an anomaly (e.g., `water_stress` via YOLO-World) and the confidence is `> 0.85`, the Edge Node will **automatically** fire the local `increase_flow` actuator (e.g., turning GPIO 17 High).
2. **Manual Dashboard Override**: 
   An operator viewing the dashboard can manually trigger physical hardware in the field using the new REST endpoint:
   ```bash
   curl -X POST http://localhost:8090/api/actuate \
     -H "Content-Type: application/json" \
     -d '{
           "node_id": "tower-bwari-alpha",
           "action": "increase_flow",
           "target": "zone_3",
           "intensity": 1.0,
           "confidence": 1.0,
           "domain": "agricultural",
           "rationale": "Manual dashboard override by operator"
         }'
   ```
   **Signal Flow**:
   Dashboard `POST` -> Python `server.py` -> publishes to MQTT topic `caesar/commands/tower-bwari-alpha` -> Edge Node async subscriber reads it -> `ActuatorBus::dispatch` -> GPIO 17 goes HIGH -> **Physical Tap Opens**.
