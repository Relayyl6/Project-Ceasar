# Project Caesar: Specific Function Assignment

This sovereign index maps precise logical capabilities to their exact file implementations. It ensures all 12 claims of the system gap analysis are explicitly accounted for in the source tree.

## 1. Rust Crates (`crates/`)

### `uriel-edge-node/src/` (Edge Daemon)
- **`main.rs`**: The primary orchestrator. Bootstraps the `SensorBus`, spawns concurrent AI worker tasks, subscribes to MQTT remote configuration commands, integrates live MAVLink GPS overrides, signs `FusedTrack` payloads, and dispatches them via the mesh `Uplink`.
- **`config.rs`**: Parses TOML. Contains the `validate_local_endpoint(url)` function which mathematically enforces Data Sovereignty by rejecting all public/cloud IPs.
- **`uplink.rs`**: The transport layer. Implements `spawn_gossipsub_swarm()`, building the `libp2p` mesh network with mDNS discovery, Noise encryption, and Strict validation. Handles TCP fallbacks.
- **`drone.rs`**: The VTOL integration. `spawn_mavlink_reader()` runs a blocking serial thread to read ArduPilot packets, injecting live `latitude_deg` into the fusion engine.
- **`dds_bridge.rs`**: The ROS 2 bridge. Initializes a `zenoh::Session` to natively publish tracks to robotics networks.
- **`fusion.rs`**: The `FusionEngine`. Receives disparate optical, thermal, and radar `Observation` events and collapses them into a `FusedTrack` based on a tight `fusion_window_ms`.
- **`inference.rs`**: The edge AI runtime. Wraps `ort` (ONNX Runtime) and `ollama`. Calculates and attaches explicit `inference_latency_ms` metrics to the payload.

### `caesar-hub/src/` (Central Hub)
- **`server.rs`**: The ingestion gateway. Parses incoming `SignedEnvelope` JSON lines and rigorously validates Ed25519 signatures against the `trusted_public_keys` config allowlist.
- **`store.rs`**: Spatial intelligence. Executes the Haversine distance formula to correlate tracks from separate nodes into a unified `CorrelatedTrack` (bounds: 500m distance, 30s temporal gap).

### `uriel-caesar-core/src/` (Shared Types)
- **`protocol.rs`**: The data contract. Defines `Modality`, `Observation`, `FusedTrack`, and `SignedEnvelope`. Centralized to prevent deserialization drift between edge nodes and the hub.

## 2. Python Services (`services/`)

### `caesar_console/` (Command Dashboard)
- **`server.py`**: The monolithic dashboard UI backend.
  - Generates the UI by serving HTML/CSS/JS.
  - Calculates `avg_inference_latency_ms` to ensure tactical SLAs are met.
  - Maintains long-lived Server-Sent Events (SSE) connections to push live threat data to the browser.
  - Proxies edge node MJPEG video streams safely via the hub.

### `mesh_orchestrator/` (Autonomous Planning)
- **`ast_planner.py`**: Adaptive Selective Tilling logic. Consumes agricultural anomaly boxes and mathematically generates a Boustrophedon sweep grid (`.plan` format) for drone flight controllers.
- **`fed_aggregator.py`**: Federated Learning logic. Aggregates statistical confidence distributions from all edge nodes (`fed_contributions/`) and computes a global `global_model.json` threshold, ensuring node knowledge is shared without moving raw images.

*See [Architecture](architecture.md) for how these files interact as a unified system.*
