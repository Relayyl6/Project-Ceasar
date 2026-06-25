# Project Caesar: Build & Deployment Plan

This document outlines the strict compilation procedures required to deploy Caesar to physical hardware.

## 1. System Requirements & Toolchain
- **Hub Environment**: x86-64 Linux (Ubuntu 22.04+ recommended).
- **Edge Environment**: ARMv8 Linux (Raspberry Pi OS Bookworm 64-bit).
- **Toolchains**: Rust stable (1.76+) and Python 3.10+.
- **System Packages**: `build-essential`, `cmake`, `pkg-config`, `libssl-dev`, `libopencv-dev`, `libclang-dev`. 
  - *Note: `libopencv-dev` is mandatory for building the edge node due to the OpenCV bindings required by the Optical inference sentinel.*

## 2. Compilation Strategy

### The Hub Server
The Hub does not require the heavy OpenCV bindings, allowing for a standard native build.
```bash
cargo build --release -p caesar-hub
```

### The Edge Node
Because compiling OpenCV natively on a Raspberry Pi 3 takes an excessive amount of time, cross-compilation from the Hub PC is highly recommended.

```bash
# Recommended: Cross-Compilation using the 'cross' crate
cross build --release --target aarch64-unknown-linux-gnu -p uriel-edge-node

# Fallback: Native Compilation (Slow)
cargo build --release -p uriel-edge-node
```

### Edge Node Feature Flags
The edge node conditionally compiles heavy dependencies to preserve RAM on lightweight field hardware.
- `--features mavlink`: Compiles `drone.rs`. Connects via UART to read ArduPilot `GLOBAL_POSITION_INT` messages, overriding the node's static coordinates.
- `--features dds`: Compiles `dds_bridge.rs`. Statically links the `zenoh` DDS router to allow native publish/subscribe interoperability with ROS 2 robotics swarms.

## 3. Python Virtual Environments
To ensure zero system-level package conflicts, Python dependencies are strictly isolated.

**Hub Console Environment:**
```bash
python3 -m venv .venv-console
source .venv-console/bin/activate
pip install paho-mqtt Pillow
```

## 4. Execution Directives
Data is strictly partitioned locally under `output/caesar/` to honor the Zero Cloud Mandate.

**1. Launch Hub:**
```bash
./target/release/caesar-hub --config configs/hub-dev.toml serve
```

**2. Launch Dashboard Console:**
```bash
source .venv-console/bin/activate
python services/caesar_console/server.py --port 8000 --mqtt-broker localhost
```

**3. Launch Edge Nodes:**
```bash
./target/release/uriel-edge-node --config configs/edge-drone-relay-01.toml
```

*See [Architecture](architecture.md) for how these binaries interact over the network.*
