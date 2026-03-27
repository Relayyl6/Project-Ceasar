# Uriel Caesar System Workspace

This repository is a safe, implementation-focused prototype of the architecture described in:

- `Project Uriel_ Mesh Intelligence Network Design.docx`
- `Project Caesar_ Augmented AI Proposal.docx`

It now goes beyond a single tracer-bullet node and includes:

- a shared protocol and crypto layer
- a Uriel edge node runtime
- a Caesar regional hub
- a mesh orchestrator for multi-node aggregation and planning
- a Caesar operator console and API
- a ROS 2 bridge package
- deployment/bootstrap helpers
- a free YOLOv8 ONNX model wired into the edge inference path

The system is still a prototype, but it now represents a coherent multi-node stack rather than only a single-node demo.

## Safety boundary

This codebase intentionally implements only the safe parts of the project concepts:

- authorized sensing
- local inference
- semantic data minimization
- signed transport
- regional aggregation
- orchestration and governance planning
- operator APIs and dashboards

This codebase does **not** implement:

- covert interception
- silent host enrollment
- unauthorized telemetry extraction
- stealth eBPF surveillance tooling

Those parts were present in the concept docs, but were deliberately left out.

## What has been built so far

### 1. Shared core layer

The shared Rust crate in [crates/uriel-caesar-core/src/lib.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-caesar-core/src/lib.rs) provides:

- signed envelope types
- fused track types
- shared serialization
- Ed25519 signing and verification
- config/file IO helpers

Important files:

- [crates/uriel-caesar-core/src/protocol.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-caesar-core/src/protocol.rs)
- [crates/uriel-caesar-core/src/crypto.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-caesar-core/src/crypto.rs)
- [crates/uriel-caesar-core/src/io.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-caesar-core/src/io.rs)

### 2. Uriel edge runtime

The edge runtime in [crates/uriel-edge-node/src/main.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-edge-node/src/main.rs) now supports:

- optical capture
- thermal adapter ingest
- radar adapter ingest
- per-modality inference workers
- fusion into semantic tracks
- signed uplink to Caesar

Important files:

- [crates/uriel-edge-node/src/camera.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-edge-node/src/camera.rs)
- [crates/uriel-edge-node/src/sensors.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-edge-node/src/sensors.rs)
- [crates/uriel-edge-node/src/inference.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-edge-node/src/inference.rs)
- [crates/uriel-edge-node/src/fusion.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-edge-node/src/fusion.rs)
- [crates/uriel-edge-node/src/uplink.rs](/C:/Users/Leonard/Documents/New project/crates/uriel-edge-node/src/uplink.rs)

Implemented optical source modes:

- `synthetic`
- `file`
- `command_stdout`
- `profile_stdout`

Implemented optical profiles:

- Raspberry Pi CSI via `rpi_csi_jpeg`
- Arducam / V4L2 / `ffmpeg` via `arducam_v4l2_ffmpeg`

Implemented inference modes:

- `heuristic`
- `command_json`

Implemented uplink modes:

- `stdout`
- `file`
- `tcp_jsonl`

### 3. Real detector path

The repository now includes a real free YOLOv8 ONNX model:

- [models/yolov8n.onnx](/C:/Users/Leonard/Documents/New project/models/yolov8n.onnx)

The external detector hook is implemented in:

- [scripts/onnx_hook.py](/C:/Users/Leonard/Documents/New project/scripts/onnx_hook.py)

It supports:

- YOLOv8-style ONNX outputs
- preprocessing with Pillow + NumPy
- ONNX Runtime execution
- class scoring and NMS
- conversion of detections into the edge node command contract

Python requirements for the detector path are listed in:

- [requirements-edge.txt](/C:/Users/Leonard/Documents/New project/requirements-edge.txt)

### 4. Caesar regional hub

The hub in [crates/caesar-hub/src/main.rs](/C:/Users/Leonard/Documents/New project/crates/caesar-hub/src/main.rs) accepts signed envelopes and persists them.

It provides:

- TCP ingest
- signature verification
- trusted-key allowlisting
- append-only journal
- latest-track snapshot
- high-interest stream

Important files:

- [crates/caesar-hub/src/server.rs](/C:/Users/Leonard/Documents/New project/crates/caesar-hub/src/server.rs)
- [crates/caesar-hub/src/store.rs](/C:/Users/Leonard/Documents/New project/crates/caesar-hub/src/store.rs)
- [configs/hub-dev.toml](/C:/Users/Leonard/Documents/New project/configs/hub-dev.toml)

### 5. Multi-node orchestration and document-layer expansion

To integrate the missing layers from the Uriel/Caesar documents, the repo now includes a control-plane service:

- [services/mesh_orchestrator/orchestrator.py](/C:/Users/Leonard/Documents/New project/services/mesh_orchestrator/orchestrator.py)

This service reads live hub outputs and produces:

- node registry
- regional summary
- orchestration plan
- governance audit log
- supervised learning plan
- semi-supervised learning plan
- reinforcement learning plan
- federated round metadata

Generated files go to:

- [output/caesar/control_plane/node_registry.json](/C:/Users/Leonard/Documents/New project/output/caesar/control_plane/node_registry.json)
- [output/caesar/control_plane/regional_summary.json](/C:/Users/Leonard/Documents/New project/output/caesar/control_plane/regional_summary.json)
- [output/caesar/control_plane/orchestration_plan.json](/C:/Users/Leonard/Documents/New project/output/caesar/control_plane/orchestration_plan.json)
- [output/caesar/control_plane/learning_plan.json](/C:/Users/Leonard/Documents/New project/output/caesar/control_plane/learning_plan.json)
- [output/caesar/control_plane/governance_audit.jsonl](/C:/Users/Leonard/Documents/New project/output/caesar/control_plane/governance_audit.jsonl)

This is the current implementation of the document concepts around:

- multiple cooperating nodes
- regional aggregation
- orchestration
- governance
- SL / USL / RL planning
- federated model round planning

### 6. Caesar console

The operator-facing service lives in:

- [services/caesar_console/server.py](/C:/Users/Leonard/Documents/New project/services/caesar_console/server.py)

It exposes:

- `/healthz`
- `/api/latest`
- `/api/journal`
- `/api/high-interest`
- `/api/stats`
- `/api/regional-summary`
- `/api/orchestration`
- `/api/learning-plan`
- `/api/node-registry`

The dashboard assets are:

- [services/caesar_console/static/index.html](/C:/Users/Leonard/Documents/New project/services/caesar_console/static/index.html)
- [services/caesar_console/static/app.js](/C:/Users/Leonard/Documents/New project/services/caesar_console/static/app.js)
- [services/caesar_console/static/styles.css](/C:/Users/Leonard/Documents/New project/services/caesar_console/static/styles.css)

### 7. ROS 2 bridge

The ROS 2 bridge package lives under:

- [ros2_ws/src/caesar_bridge/package.xml](/C:/Users/Leonard/Documents/New project/ros2_ws/src/caesar_bridge/package.xml)
- [ros2_ws/src/caesar_bridge/caesar_bridge/bridge_node.py](/C:/Users/Leonard/Documents/New project/ros2_ws/src/caesar_bridge/caesar_bridge/bridge_node.py)

It tails the Caesar high-interest feed and republishes records into ROS 2 as `std_msgs/String`.

### 8. Deployment helpers

Bootstrap scripts:

- [scripts/bootstrap_edge_pi.sh](/C:/Users/Leonard/Documents/New project/scripts/bootstrap_edge_pi.sh)
- [scripts/bootstrap_hub.sh](/C:/Users/Leonard/Documents/New project/scripts/bootstrap_hub.sh)
- [scripts/bootstrap_ros2_humble.sh](/C:/Users/Leonard/Documents/New project/scripts/bootstrap_ros2_humble.sh)

Key and model tooling:

- [scripts/manage_keys.py](/C:/Users/Leonard/Documents/New project/scripts/manage_keys.py)
- [scripts/fetch_free_model.py](/C:/Users/Leonard/Documents/New project/scripts/fetch_free_model.py)

Example service units:

- [deploy/systemd/uriel-edge-node.service](/C:/Users/Leonard/Documents/New project/deploy/systemd/uriel-edge-node.service)
- [deploy/systemd/caesar-hub.service](/C:/Users/Leonard/Documents/New project/deploy/systemd/caesar-hub.service)
- [deploy/systemd/caesar-console.service](/C:/Users/Leonard/Documents/New project/deploy/systemd/caesar-console.service)
- [deploy/systemd/caesar-orchestrator.service](/C:/Users/Leonard/Documents/New project/deploy/systemd/caesar-orchestrator.service)

## Repository structure

Top-level structure:

- [Cargo.toml](/C:/Users/Leonard/Documents/New project/Cargo.toml)
- [Cargo.lock](/C:/Users/Leonard/Documents/New project/Cargo.lock)
- [configs/](/C:/Users/Leonard/Documents/New project/configs)
- [crates/](/C:/Users/Leonard/Documents/New project/crates)
- [services/](/C:/Users/Leonard/Documents/New project/services)
- [scripts/](/C:/Users/Leonard/Documents/New project/scripts)
- [models/](/C:/Users/Leonard/Documents/New project/models)
- [ros2_ws/](/C:/Users/Leonard/Documents/New project/ros2_ws)
- [deploy/](/C:/Users/Leonard/Documents/New project/deploy)

Config files currently included:

- [configs/edge-dev.toml](/C:/Users/Leonard/Documents/New project/configs/edge-dev.toml)
- [configs/edge-pi.toml](/C:/Users/Leonard/Documents/New project/configs/edge-pi.toml)
- [configs/edge-v4l2.toml](/C:/Users/Leonard/Documents/New project/configs/edge-v4l2.toml)
- [configs/edge-bwari-alpha.toml](/C:/Users/Leonard/Documents/New project/configs/edge-bwari-alpha.toml)
- [configs/edge-bwari-bravo.toml](/C:/Users/Leonard/Documents/New project/configs/edge-bwari-bravo.toml)
- [configs/edge-drone-relay-01.toml](/C:/Users/Leonard/Documents/New project/configs/edge-drone-relay-01.toml)
- [configs/hub-dev.toml](/C:/Users/Leonard/Documents/New project/configs/hub-dev.toml)
- [configs/mesh-cluster.json](/C:/Users/Leonard/Documents/New project/configs/mesh-cluster.json)

## Mapping from the documents to this repo

This section explains how the current code maps to the larger concepts in the two project documents.

### Uriel document concepts implemented

- edge-computed local inference
- semantic payload minimization instead of raw sensor backhaul
- multi-sensor fusion
- multiple node roles
- fixed tower and drone relay node profiles
- signed transport and zero-trust style node identity
- multi-frequency / multi-protocol planning represented in config and orchestration output
- regional aggregation and governance planning

### Caesar document concepts implemented

- cognitive edge layer
- aggregation layer
- orchestration layer
- governance/security layer
- supervised learning planning
- semi-supervised anomaly planning
- reinforcement learning routing-policy planning
- federated round planning
- protocol diversity represented in cluster policy and routing outputs

### Concepts represented as planning/output rather than full execution

These concepts are present, but not fully executed as live learning/training systems yet:

- federated training execution
- Distributed Gaussian Process anomaly modeling
- MARL runtime policy optimization
- live Zenoh / AMQP / MQTT data-plane transport
- automatic dynamic policy updates back to the nodes

Those are represented in:

- [configs/mesh-cluster.json](/C:/Users/Leonard/Documents/New project/configs/mesh-cluster.json)
- [services/mesh_orchestrator/orchestrator.py](/C:/Users/Leonard/Documents/New project/services/mesh_orchestrator/orchestrator.py)
- [output/caesar/control_plane/learning_plan.json](/C:/Users/Leonard/Documents/New project/output/caesar/control_plane/learning_plan.json)
- [output/caesar/control_plane/orchestration_plan.json](/C:/Users/Leonard/Documents/New project/output/caesar/control_plane/orchestration_plan.json)

### Concepts intentionally excluded

- covert interception
- stealth eBPF probes
- silent enrollment
- unauthorized host extraction

## Multi-node topology currently modeled

The current cluster model in [configs/mesh-cluster.json](/C:/Users/Leonard/Documents/New project/configs/mesh-cluster.json) includes:

- `tower-bwari-alpha`
- `tower-bwari-bravo`
- `drone-relay-01`
- `hub-bwari-01`

Roles currently represented:

- `fixed_tower`
- `relay`
- `regional_hub`

Learning-layer mapping currently represented:

- `sl`
- `usl`
- `rl`

Protocol families currently represented:

- `dds`
- `zenoh`
- `mqtt`
- `amqp`

## Hardware assumptions

### Edge node hardware

Recommended current edge target:

- Raspberry Pi 5, 8 GB
- active cooling
- stable power
- Raspberry Pi Camera Module 3 or Arducam/V4L2 camera
- optional thermal device
- optional radar device

### Hub hardware

- Ubuntu/Linux workstation, mini PC, or server
- stable network access
- enough disk for journals and snapshots

### ROS machine

- Ubuntu 22.04
- ROS 2 Humble

## What is real today vs what is simulated

### Real today

- optical ingest through Pi CSI or V4L2 command pipelines
- YOLOv8 ONNX detector path
- signed edge-to-hub envelopes
- trusted public-key enforcement
- hub journaling and snapshot storage
- regional control-plane generation
- console API/dashboard
- ROS 2 bridge package

### Simulated or adapter-based today

- thermal sensor reader
- radar reader
- federated model exchange
- reinforcement policy execution
- live multi-protocol data plane beyond the current TCP JSONL transport

The thermal and radar paths are designed to be replaced with vendor-specific wrappers using the JSON adapter contracts.

## How to run the system

### A. Local synthetic multi-node demo

This is the easiest way to exercise the structure without hardware.

1. Start the hub:

```powershell
cargo run -p caesar-hub -- --config configs/hub-dev.toml serve
```

2. In separate terminals, start one or more synthetic edge nodes:

```powershell
cargo run -p uriel-edge-node -- --config configs/edge-bwari-alpha.toml
```

```powershell
cargo run -p uriel-edge-node -- --config configs/edge-bwari-bravo.toml
```

```powershell
cargo run -p uriel-edge-node -- --config configs/edge-drone-relay-01.toml
```

3. Start the orchestrator:

```powershell
python services/mesh_orchestrator/orchestrator.py --interval 15
```

4. Start the console:

```powershell
python services/caesar_console/server.py --host 127.0.0.1 --port 8090
```

5. Open:

```text
http://127.0.0.1:8090
```

### B. Raspberry Pi edge deployment

Use:

- [configs/edge-pi.toml](/C:/Users/Leonard/Documents/New project/configs/edge-pi.toml)

Steps:

1. Install Ubuntu 22.04 64-bit on the Pi.
2. Clone the repo to `/opt/uriel-caesar`.
3. Run:

```bash
bash scripts/bootstrap_edge_pi.sh /opt/uriel-caesar
```

4. Connect the Raspberry Pi CSI camera ribbon.
5. Edit `configs/edge-pi.toml`:

- set the hub IP in `uplink.tcp_addr`
- set the physical site coordinates
- confirm the model path
- replace thermal/radar adapter commands if real devices are available

6. Start the edge node:

```bash
cargo run -p uriel-edge-node -- --config configs/edge-pi.toml
```

### C. V4L2 / Arducam edge deployment

Use:

- [configs/edge-v4l2.toml](/C:/Users/Leonard/Documents/New project/configs/edge-v4l2.toml)

Before running:

- connect the Arducam / USB camera
- confirm the actual `/dev/video*` device
- confirm the supported pixel format

Then run:

```bash
cargo run -p uriel-edge-node -- --config configs/edge-v4l2.toml
```

### D. Hub deployment

1. Clone the repo to `/opt/uriel-caesar`.
2. Run:

```bash
bash scripts/bootstrap_hub.sh /opt/uriel-caesar
```

3. Start the hub:

```bash
cargo run -p caesar-hub -- --config configs/hub-dev.toml serve
```

4. Start the console:

```bash
python services/caesar_console/server.py --host 0.0.0.0 --port 8090
```

5. Start the orchestrator:

```bash
python services/mesh_orchestrator/orchestrator.py --interval 15
```

### E. ROS 2 bridge deployment

1. Prepare the ROS machine:

```bash
bash scripts/bootstrap_ros2_humble.sh
```

2. Build the workspace:

```bash
cd ros2_ws
source /opt/ros/humble/setup.bash
colcon build
source install/setup.bash
ros2 run caesar_bridge bridge_node
```

## Key management

The hub currently trusts the configured edge node seeds through:

- [configs/hub-dev.toml](/C:/Users/Leonard/Documents/New project/configs/hub-dev.toml)

To derive or generate keys:

```powershell
.\\.venv-local-tools\\Scripts\\python.exe scripts\\manage_keys.py derive --seed-hex 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff
```

or:

```powershell
.\\.venv-local-tools\\Scripts\\python.exe scripts\\manage_keys.py generate --hub-config configs\\hub-dev.toml
```

## Model management

The repository already contains:

- [models/yolov8n.onnx](/C:/Users/Leonard/Documents/New project/models/yolov8n.onnx)

If you want to export a fresh free model with Ultralytics on a Linux/Pi host:

```powershell
python scripts/fetch_free_model.py --variant yolov8n --repo-root .
```

## Testing and validation status

Validation run noted here reflects work completed on **March 27, 2026**.

### Tests completed successfully on this workstation

- `cargo metadata --format-version 1 --no-deps`
- `cargo fmt --all --check`
- Python syntax checks for:
  - console service
  - orchestrator service
  - key and model tools
  - ROS bridge files
  - adapter scripts
- `services/mesh_orchestrator/orchestrator.py --run-once`
- control-plane file generation under `output/caesar/control_plane`
- console HTTP smoke test against:
  - `/healthz`
  - `/api/stats`
  - `/api/regional-summary`
- key derivation via `scripts/manage_keys.py`
- model presence check for `models/yolov8n.onnx`

### Tests not fully possible on this workstation

- full Rust build and `cargo check`
- running the Rust hub binary
- running the Rust edge binary
- full end-to-end edge -> hub -> orchestrator -> console -> ROS with live hardware

### Why those tests are blocked

This Windows workstation is missing the MSVC linker `link.exe`, so Rust compilation cannot complete here. The repo is structurally valid, but real Rust execution needs:

- Visual Studio Build Tools with C++, or
- a Linux/Pi deployment target

## Known limitations

- Thermal and radar readers are still adapter contracts rather than vendor-specific drivers.
- Multi-protocol transport is represented in cluster/orchestration planning, but the live data plane currently uses the implemented TCP JSONL uplink path.
- Federated learning, anomaly training, and reinforcement updates are currently planning/control-plane outputs, not active distributed training loops.
- ROS 2 bridge currently publishes journal records as `std_msgs/String`, not custom message types.

## Recommended next implementation steps

If development continues, the highest-value next steps are:

1. Replace thermal adapter stubs with real vendor readers.
2. Replace radar adapter stubs with real vendor readers.
3. Move the hub and edge services to Linux and complete real Rust builds.
4. Add a launcher for bringing up hub, orchestrator, console, and multiple nodes together.
5. Replace `std_msgs/String` in ROS with custom typed messages.
6. Add real federated model artifact exchange instead of planning-only rounds.

## Supporting docs

- [services/caesar_console/README.md](/C:/Users/Leonard/Documents/New project/services/caesar_console/README.md)
- [services/mesh_orchestrator/README.md](/C:/Users/Leonard/Documents/New project/services/mesh_orchestrator/README.md)
- [ros2_ws/README.md](/C:/Users/Leonard/Documents/New project/ros2_ws/README.md)
- [models/README.md](/C:/Users/Leonard/Documents/New project/models/README.md)
