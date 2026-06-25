# Project Caesar: UI Registry & API Surface

This document logs every Front-End component, API Endpoint, and DOM ID required to construct the Caesar Console. It details how the python backend feeds the glassmorphic tactical UI.

## 1. REST API Endpoints (Hub -> Console)
These endpoints are served natively by the Python `HTTPServer` in `server.py`.

### Telemetry & State
- **`GET /api/stats`**: Returns the global operational state. Parses `inference_latency_ms` from edge tracks to calculate `avg_inference_latency_ms` and `sla_sub100ms_pct`.
- **`GET /api/anomaly-log`**: Returns historical `high_interest` tracks.
- **`GET /api/alert-stream`**: Holds a long-lived `text/event-stream` (SSE) connection open. When the hub's Rust daemon detects an anomaly, Python pushes the JSON string prefixed with `data: ` to the browser for instant rendering.

### Video Routing
- **`GET /api/proxy-camera?node={id}`**: Routes live video from edge drones directly to the dashboard. Python opens an `urllib.request` to the edge node's internal MJPEG port and safely pipes the `multipart/x-mixed-replace` chunks to the browser.
- **`GET /api/camera-stream`**: Fallback endpoint streaming the local hub camera if the edge node disconnects.

### Command & Control
- **`POST /api/reconfigure`**: Allows operators to remote-tune node parameters. The Python server publishes a payload (e.g., `{"node_id": "...", "patch": {"threat_threshold": 0.85}}`) to the MQTT broker. The edge Rust daemon safely hot-swaps the atomic variables.
- **`POST /api/actuate`**: Manual hardware overrides (e.g., triggering a relay). Routes over MQTT using type `"actuate"`.
- **`POST /api/fed/contribute`**: The ingest endpoint where edge nodes POST their local model confidence distributions for the Federated ML system to process.

## 2. Essential DOM Identifiers
The Vanilla JS application (`app.js`) strictly binds to these elements to execute DOM mutations without reloading the page.
- **`#alert-feed`**: The container where the SSE alert stream appends new glassmorphic anomaly cards.
- **`#camera-view`**: The `<img>` tag receiving the `/api/proxy-camera` `src` injection.
- **`#stats-latency-avg`**: Span holding the real-time `avg_inference_latency_ms`.
- **`#stats-sla-pct`**: Span holding the system SLA compliance metric.
- **`#btn-reconfigure`**: Button triggering the remote configuration modal.

*See [UI Rules](ui-rules.md) for the aesthetic requirements binding these components.*
