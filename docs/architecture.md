# Project Caesar: Architecture

## 1. System Topology
Project Caesar operates as a highly distributed, sovereign mesh network. It abandons traditional cloud-star topologies in favor of a resilient edge-to-hub architecture.

### 1.1 The Edge (`uriel-edge-node`)
The Rust-based edge daemon deployed on field hardware (Raspberry Pi, drones).
- **Sensors**: Ingests data via `SensorBus` from optical (V4L2), thermal (I2C), and radar (UART) arrays.
- **Inference**: Executes ONNX models (YOLO-World, pxADMM) entirely locally. Protected by strict `tokio::time::timeout` boundaries (100ms for YOLO, 300ms for VLM) to prevent frame starvation and guarantee tactical latency.
- **Fusion**: The `FusionEngine` unifies asynchronous modal observations into a single `FusedTrack`.
- **Telemetry**: Integrates directly with ArduPilot/PX4 flight controllers via MAVLink (`drone.rs`) to inject live GPS coordinates.
- **Transport**: Encapsulates data into a cryptographic `SignedEnvelope` and uplinks it via the `libp2p` Gossipsub mesh or Zenoh DDS.

### 1.2 The Hub (`caesar-hub`)
The terrestrial aggregation point.
- **Validation**: Rejects all unsigned or unauthorized envelopes via an Ed25519 allowlist.
- **Correlation**: Merges overlapping tracks from disparate edge nodes using Haversine distance (500m) and temporal (30s) bounds, yielding a unified `CorrelatedTrack`.
- **Real-Time Event Bus**: Binds a pure-Rust ZeroMQ PUB socket to instantly stream correlated tracks to the orchestrator with sub-50ms latency.
- **Secure Command Gateway**: Ingests autonomous response payloads, performs mathematical Geo-fencing validation (rejecting out-of-bounds coordinates), and forwards valid commands back to the edge via MQTT.

### 1.3 The Orchestration Layer
- **Autonomous Threat Escalation**: An `asyncio` event loop that digests the ZeroMQ stream, assigning threat levels dynamically.
- **Multi-Camera Tracking Engine**: Uses a Kalman Filter for kinematic velocity prediction and the Hungarian Algorithm for cross-camera target assignment. Instantly dispatches drones/PTZ commands based on predicted routes.
- **AST Planning**: `ast_planner.py` consumes agricultural anomalies and generates Boustrophedon survey flight paths for drones.
- **Federated ML**: `fed_aggregator.py` computes global confidence thresholds from localized edge distributions without ever centralizing raw imagery.

## 2. Network Protocols
- **libp2p Gossipsub**: The primary mesh protocol. Edge nodes auto-discover via mDNS and route `SignedEnvelope` packets securely without a central broker.
- **Zenoh DDS**: The bridge protocol. Exposes Caesar tracks to standard ROS 2 robotics networks.
- **MQTT**: The command protocol. Allows the `caesar_console` to dispatch hot-swappable configuration patches (e.g., `threat_threshold`) down to live edge nodes.

*See [Project Overview](project-overview.md) for the mission context and [Library Docs](library-docs.md) for the exact dependency stack driving this architecture.*
