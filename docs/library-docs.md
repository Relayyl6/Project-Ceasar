# Project Caesar: Library & Dependency Inventory

To maintain Data Sovereignty and system performance, Project Caesar strictly limits third-party libraries. This document lists the exact permitted libraries and their roles.

## 1. Rust Ecosystem (`Cargo.toml`)

### Asynchronous Execution & Networking
- **`tokio` (v1.35)**: The core async runtime. Powers all non-blocking network I/O and concurrent AI pipelines.
- **`libp2p` (v0.53)**: The backbone of the Caesar Mesh. 
  - Features utilized: `gossipsub` (topic-routing), `mdns` (local discovery), `noise` (encryption), `tcp`, `yamux`.
- **`zenoh` (v1.0)**: The ROS 2 bridge protocol. Used exclusively in `dds_bridge.rs` for robotic interop.
- **`rumqttc` (v0.25.1)**: Lightweight async MQTT client used by edge nodes to receive remote tuning commands from the dashboard.

### Core Compute & Sensor Fusion
- **`ort` (v2.0.0-rc.2)**: Rust bindings for ONNX Runtime. Enables the edge node to run neural networks (YOLO-World, pxADMM) natively without a Python interpreter.
- **`opencv` (v0.93)**: Rust bindings for OpenCV. Drives the Optical Sentinel, executing rapid frame differencing to gate heavy AI inference and save CPU cycles.
- **`mavlink` (v0.13)**: The VTOL drone integration library. Parses raw telemetry from flight controllers.

### Cryptography & Payload Structure
- **`ed25519-dalek` (v2.1)**: Underpins the sovereign identity system. Generates the cryptographic key pairs used to sign all `FusedTrack` payloads.
- **`serde` (v1.0.219)** & **`serde_json` (v1.0.140)**: The universal serialization framework for all REST, P2P, and File IO.
- **`blake3` (v1.5)**: Extremely fast hashing used to generate forensic evidence digests for tracked anomalies.

## 2. Python Ecosystem (Hub Services)

### Standard Library (No `pip install` required)
- **`http.server`**: Runs the Caesar Console UI, adhering to our rule against heavy web frameworks.
- **`urllib.request`**: Handles the direct proxying of MJPEG video streams from edge nodes to the browser.
- **`pathlib`**: Handles all journal file I/O safely.

### External Libraries (`pip`)
- **`paho-mqtt`**: Used by the dashboard to publish remote configuration patches to the edge nodes.
- **`numpy` (>=1.26)**: Executes matrix math for Autonomous Planning (`ast_planner.py` boustrophedon grids) and Federated Averaging (`fed_aggregator.py`).
- **`Pillow` (>=10.0)**: Processes optical anomaly portraits before rendering them to the dashboard.

*See [Code Standards](code-standards.md) for how we handle dependencies and error propagation within these libraries.*
