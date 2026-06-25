# Project Caesar: Code Standards & Development Rules

These standards ensure the system remains sovereign, robust, and mathematically sound.

## 1. Absolute Data Sovereignty Enforcement
Caesar must never accidentally or maliciously route data to public clouds.
- **The Rule**: `validate_local_endpoint(url: &str) -> bool` (in `config.rs`) must wrap all outbound HTTP clients and sockets.
- **Accepted Ranges**: `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, and `.local` or `.lan` domains.
- Any attempt to reach an IP outside these RFC 1918 / RFC 4193 bounds must intentionally panic at startup or cleanly reject the packet at runtime. No exceptions.

## 2. Rust Codebase Standards (Core, Edge, Hub)
Rust code guarantees memory safety, but we must also guarantee thread responsiveness.
- **Asynchronous Execution (`tokio`)**: 
  - Network I/O (TCP, Gossipsub, HTTP) uses `tokio::spawn`.
  - CPU-bound AI Inference (ONNX) uses `tokio::task::spawn_blocking` so the network executor does not stall.
  - Blocking Hardware I/O (Linux Serial UARTs like MAVLink) use standard OS threads (`std::thread::spawn`) to isolate unpredictable kernel drivers.
- **Error Handling**: `anyhow::Result` is mandated for all fallible functions. `.context("Descriptive message")` must trace the error path. Production code must not `unwrap()` unless initializing immutable constants.
- **Data Sharing**: Prefer `tokio::sync::mpsc` channels for data pipelines (e.g., passing sensor frames). If shared state is unavoidable, use `Arc<RwLock<T>>`.
- **Serialization Compatibility**: All network payloads (defined in `protocol.rs`) must use `#[serde(default)]` when adding new fields (e.g., `inference_latency_ms`) to prevent protocol breakage between older edge nodes and newer hubs.

## 3. Python Services Standards
Python is restricted strictly to the UI layer and orchestration batch-jobs.
- **Frameworks**: No heavy web frameworks (Django/Flask). The UI runs cleanly on the standard library `http.server` to minimize dependency footprints on tactical hardware.
- **Security**: 
  - Max POST bodies must be capped at 64KB to prevent memory exhaustion DoS.
  - Input parsing (e.g., reading `node_id`) must be strictly sanitized via regex (`^[A-Za-z0-9_-]{1,64}$`) to prevent directory traversal attacks during journal writes.
- **Typing**: Python must use strict type hints for all functions.

*See [Architecture](architecture.md) for how these rules manifest across the Edge and Hub components.*
