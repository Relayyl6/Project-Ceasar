# Hardware Transition & Placeholder Identification Plan

This document identifies all simulation logic, dummy data, and `println!` placeholders currently used in the Project Caesar/Uriel edge mesh codebase. It provides explicit engineering steps required to transition these simulations to actual hardware and production APIs.

## 1. Passive Reconnaissance Engine
**File:** `crates/uriel-edge-node/src/recon.rs`
**Placeholders Identified:**
- `println!` statements are used to simulate various passive reconnaissance tasks including eBPF `sys_enter` interception, USB fingerprinting, ONVIF CCTV hijacking, `libssl.so` uprobe hooking, PCAP/BLE profiling, and Counter-Drone EW scanning.
- `sleep()` calls simulate the time taken for these scans.

**Transition to Hardware:**
- **eBPF/uprobes**: Use the `aya` and `aya-bpf` Rust crates to compile actual eBPF bytecode. Use `aya::programs::KProbe` to attach to `sys_enter` and `aya::programs::UProbe` to hook into `/usr/lib/libssl.so` to extract raw plaintext buffers.
- **USB Fingerprinting**: Use the `nusb` crate. Call `nusb::list_devices()` to fetch real physical descriptors (VID, PID, manufacturer strings) from the USB root hub instead of sleeping.
- **ONVIF CCTV**: Use the `onvif-cam-rs` crate to broadcast real WS-Discovery SOAP payloads over the local subnet, parse responding XML, and use OpenCV or GStreamer to open the discovered `rtsp://` streams.
- **PCAP/BLE & RF EW**: Bind to a WiFi interface placed in monitor mode using `libpcap` bindings (`pcap` crate) to capture raw 802.11 probe requests. For Counter-Drone RF, integrate a Software Defined Radio (e.g., HackRF or RTL-SDR) using `soapysdr-rs` to run real-time FFTs and hunt for unauthorized C2 frequencies.

---

## 2. Universal Hardware Abstraction Layer (HAL)
**File:** `crates/uriel-caesar-core/src/hal.rs`
**Placeholders Identified:**
- `SimulatedThermalSensor`, `SimulatedRadarSensor`, `SimulatedOpticalSensor`, and `SimulatedAcousticSensor` return hardcoded dummy arrays (e.g., `vec![128; 640 * 512]` or `vec![255; 12000000]`) when `read_data()` is called.

**Transition to Hardware:**
- **Sony IMX577 (Optical)**: Replace the dummy vector by interfacing with `libcamera` via Rust bindings (or calling `rpicam-vid` into a named pipe) to pull raw CSI-2 MIPI pixel matrices from the Raspberry Pi 5 camera bus.
- **FLIR Boson+ 640 (Thermal)**: Implement UART/I2C communication to read the raw 16-bit radiometric thermal matrices directly from the FLIR hardware.
- **TI IWR6843 (Radar)**: Use the `serialport` crate to open the radar's data COM port, continuously read the serial buffer, and parse the hardware's proprietary TLV (Type-Length-Value) format to extract actual 3D point clouds and velocity data.
- **Acoustic**: Interface with an I2S MEMS microphone using `cpal` or `alsa` Rust crates to stream live audio PCM buffers.

---

## 3. Sensor Integration Logic
**File:** `crates/uriel-edge-node/src/sensors.rs`
**Placeholders Identified:**
- The engine uses the `Simulated*` structs from `hal.rs` and utilizes `eprintln!` strictly for error logging when the dummy reads fail.

**Transition to Hardware:**
- Simply swap the initialization of the `Simulated*` structs to the production structs created in `hal.rs`. Ensure asynchronous buffer capacities match the high-throughput reality of the physical hardware to avoid dropped frames.

---

## 4. Spatiotemporal Fusion & Predictive Maintenance
**File:** `crates/uriel-edge-node/src/fusion.rs`
**Placeholders Identified:**
- **SCLE Isolation**: Uses `println!` to denote when a fault has been isolated by the `catch_unwind` block.
- **EKF Delta-Time**: The `ekf.predict(0.1)` function assumes a hardcoded `dt` of 100ms.
- **Z-Score Calculation**: The predictive maintenance vibration anomaly uses a simulated baseline mean of `0.018 rms` and a standard deviation of `0.005` against pseudo-random velocities.

**Transition to Hardware:**
- **EKF Delta-Time**: Track the actual `std::time::Instant` when sensor frames are received. Pass the true delta-time (e.g., `dt = now.duration_since(last_frame).as_secs_f32()`) to the EKF prediction step to accurately reflect system latency.
- **Z-Score Calculation**: Maintain a rolling window buffer (e.g., a `VecDeque` of the last 10,000 acoustic frames). Dynamically calculate the true mean and true standard deviation of the live acoustic data, updating the baseline iteratively.

---

## 5. Machine Learning Inference
**File:** `crates/uriel-edge-node/src/inference.rs`
**Placeholders Identified:**
- `OrtYoloPipeline::infer` skips tensor evaluation and returns a hardcoded `Observation` with a dummy position and confidence.
- `heuristic_optical_infer` returns pseudo-random bounding boxes based on the modulo of `frame.sequence`.
- `thermal_infer` assigns classes like `"hot-vehicle"` and `"crop-stress-early-warning"` using arbitrary temperature thresholds and modulo sequences for position.
- `radar_infer` assigns generic `"moving-object"` tracking using simple means of dummy data.

**Transition to Hardware:**
- Use the `ndarray` crate to preprocess raw image buffers (resize, normalize) and feed them into `session.run()` for `OrtYoloPipeline`. Parse the output tensors, apply Non-Maximum Suppression (NMS), and generate `Observation` structs from the actual bounding boxes.
- For thermal and radar, load the actual exported ONNX models (e.g., LSTM for crop stress) and feed the raw multi-dimensional arrays into the `ort` engine to extract the true classification confidences and threat levels.

---

## 6. Mesh Networking & Uplink
**File:** `crates/uriel-edge-node/src/uplink.rs`
**Placeholders Identified:**
- `UplinkKind::Gossipsub` uses `println!` to simulate broadcasting to a libp2p topic and simulating the RFD900x asynchronous baud rate management.

**Transition to Hardware:**
- **libp2p Gossipsub**: Construct a full `libp2p::Swarm` featuring `Noise` encryption, `Yamux` multiplexing, and the `Gossipsub` behaviour. Replace the `println!` with `swarm.behaviour_mut().gossipsub.publish(libp2p::gossipsub::IdentTopic::new(topic), payload.as_bytes())`.
- **RFD900x Baud Shaping**: Instantiate a `serialport` connection to `/dev/ttyUSB0` configured strictly to the desired baud rate (e.g., 57600). Write the JSON payload directly into the serial port stream rather than standard output, managing the backpressure natively.

---

## 7. Caesar Hub Mesh Overlay
**File:** `crates/caesar-hub/src/server.rs`
**Placeholders Identified:**
- `spawn_mesh_overlay` uses `tokio::time::sleep` and `println!` to simulate joining the peer-to-peer Gossipsub mesh and listening for incoming envelopes.

**Transition to Hardware:**
- Implement the exact inverse of the Edge Node's `libp2p` swarm. The Hub must instantiate a `libp2p::Swarm`, listen on all network interfaces (`0.0.0.0`), and continuously poll the swarm for `GossipsubEvent::Message` items. Upon receiving a message, parse it as a `SignedEnvelope` and push it to the Hub's persistent storage backend.
