import argparse
import json
import mimetypes
import time
import struct
import math
import threading
_tls = threading.local()
from collections import Counter
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import re
from urllib.parse import parse_qs, urlparse

# Maximum bytes accepted in a POST body (prevents a client from sending an
# arbitrarily large Content-Length that would block rfile.read() indefinitely).
_MAX_POST_BYTES = 64 * 1024  # 64 KiB — plenty for any actuator command

# Maximum rows that any paginated API endpoint will return.  Prevents a caller
# from sending limit=999999999 to exhaust memory reading a huge JSONL file.
_MAX_QUERY_LIMIT = 5_000

# Maximum bytes for a single structured JSON file read (node-registry,
# orchestration-plan, etc.).  Guards against an accidentally enormous file
# exhausting the server process's memory on /api/node-registry and friends.
_MAX_FILE_BYTES = 4 * 1024 * 1024  # 4 MiB

# Allowed characters in a node_id supplied over HTTP.  Rejects path traversal
# and shell-injection attempts before the value reaches PIL / filesystem code.
_NODE_ID_RE = re.compile(r'^[\w.-]{1,64}$')


def _safe_limit(qs: dict, key: str, default: int, maximum: int = _MAX_QUERY_LIMIT) -> int:
    """Parse an integer query-string parameter with a safe upper bound."""
    raw = qs.get(key, [str(default)])[0]
    try:
        value = int(raw)
    except (ValueError, OverflowError):
        value = default
    return max(1, min(value, maximum))


def _safe_node_id(raw: str) -> str:
    """Return the raw node_id if it looks safe, or 'local' as a fallback."""
    if _NODE_ID_RE.match(raw):
        return raw
    return "local"

# ── CAMERA STATE ─────────────────────────────────────────────────────────────
_camera_lock = threading.Lock()
_camera_cap = None          # OpenCV VideoCapture, if available
_camera_frame_bytes = None  # last JPEG bytes
_camera_node_id = "local"
_anomaly_log = []           # recent detection strings

# ── ALERT STATE (for real-time SSE push) ──────────────────────────────────────────
_hi_path_cache: Path = Path("output/caesar/high_interest.jsonl")  # updated in main()


def _check_alerts(hi_path: Path = None) -> dict | None:
    """
    Return the last alert dict if a new line has been appended to
    high_interest.jsonl since the last call, otherwise return None.
    """
    path = hi_path if hi_path is not None else _hi_path_cache
    
    if not hasattr(_tls, 'last_hi_size'):
        try:
            _tls.last_hi_size = path.stat().st_size if path.exists() else 0
        except OSError:
            _tls.last_hi_size = 0
            
    try:
        if not path.exists():
            return None
        sz = path.stat().st_size
        if sz <= _tls.last_hi_size:
            return None
        _tls.last_hi_size = sz
        with path.open('r', encoding='utf-8') as fh:
            lines = [l for l in fh.readlines() if l.strip()]
        if not lines:
            return None
        return json.loads(lines[-1])
    except (OSError, json.JSONDecodeError, ValueError):
        return None
# ─────────────────────────────────────────────────────────────────────────────────

def _init_camera():
    """Try to open a real webcam via OpenCV. Auto-scans devices, falls back to synthetic frames."""
    global _camera_cap
    try:
        import cv2
        import glob
        target_dev = 0
        devices = glob.glob("/dev/video*")
        if devices:
            try:
                devices.sort(key=lambda x: int(x.replace("/dev/video", "")))
                target_dev = int(devices[0].replace("/dev/video", ""))
            except ValueError:
                pass
        
        cap = cv2.VideoCapture(target_dev)
        if cap.isOpened():
            _camera_cap = cap
            print(f"[camera] Hardware webcam opened on device {target_dev}.")
        else:
            print(f"[camera] No webcam found at device {target_dev}. Using synthetic frames.")
    except ImportError:
        print("[camera] opencv-python not installed. Using synthetic frames.")

def _synthetic_jpeg(width=640, height=480, label="URIEL OPTICAL FEED"):
    """
    Returns bytes of a static NO SIGNAL image.
    This serves as an explicit fallback when the camera is offline.
    """
    try:
        from PIL import Image, ImageDraw, ImageFont
        import io, datetime
        img = Image.new("RGB", (width, height), color=(10, 10, 10))
        draw = ImageDraw.Draw(img)
        # Scanlines
        for y in range(0, height, 4):
            draw.line([(0, y), (width, y)], fill=(20, 20, 20))
        
        # Big NO SIGNAL text
        cx, cy = width // 2, height // 2
        draw.text((cx - 40, cy - 10), "NO SIGNAL", fill=(255, 51, 68))
        draw.text((cx - 65, cy + 10), "CAMERA OFFLINE / DISCONNECTED", fill=(180, 40, 50))
        
        # Timestamp and label
        ts = datetime.datetime.now().strftime("%H:%M:%S")
        draw.text((10, 10), f"URIEL-NODE / {label} (OFFLINE)", fill=(255, 51, 68))
        draw.text((10, 28), f"UTC {ts}", fill=(255, 51, 68))
        
        buf = io.BytesIO()
        img.save(buf, format="JPEG", quality=75)
        return buf.getvalue()
    except ImportError:
        # Minimal 1x1 black JPEG fallback if PIL is missing
        return bytes([
            0xFF,0xD8,0xFF,0xE0,0x00,0x10,0x4A,0x46,0x49,0x46,0x00,0x01,
            0x01,0x00,0x00,0x01,0x00,0x01,0x00,0x00,0xFF,0xDB,0x00,0x43,
            0x00,0x08,0x06,0x06,0x07,0x06,0x05,0x08,0x07,0x07,0x07,0x09,
            0x09,0x08,0x0A,0x0C,0x14,0x0D,0x0C,0x0B,0x0B,0x0C,0x19,0x12,
            0x13,0x0F,0x14,0x1D,0x1A,0x1F,0x1E,0x1D,0x1A,0x1C,0x1C,0x20,
            0x24,0x2E,0x27,0x20,0x22,0x2C,0x23,0x1C,0x1C,0x28,0x37,0x29,
            0x2C,0x30,0x31,0x34,0x34,0x34,0x1F,0x27,0x39,0x3D,0x38,0x32,
            0x3C,0x2E,0x33,0x34,0x32,0xFF,0xC0,0x00,0x0B,0x08,0x00,0x01,
            0x00,0x01,0x01,0x01,0x11,0x00,0xFF,0xC4,0x00,0x1F,0x00,0x00,
            0x01,0x05,0x01,0x01,0x01,0x01,0x01,0x01,0x00,0x00,0x00,0x00,
            0x00,0x00,0x00,0x00,0x01,0x02,0x03,0x04,0x05,0x06,0x07,0x08,
            0x09,0x0A,0x0B,0xFF,0xC4,0x00,0xB5,0x10,0x00,0x02,0x01,0x03,
            0x03,0x02,0x04,0x03,0x05,0x05,0x04,0x04,0x00,0x00,0x01,0x7D,
            0x01,0x02,0x03,0x00,0x04,0x11,0x05,0x12,0x21,0x31,0x41,0x06,
            0x13,0x51,0x61,0x07,0x22,0x71,0x14,0x32,0x81,0x91,0xA1,0x08,
            0x23,0x42,0xB1,0xC1,0x15,0x52,0xD1,0xF0,0x24,0x33,0x62,0x72,
            0x82,0x09,0x0A,0x16,0x17,0x18,0x19,0x1A,0x25,0x26,0x27,0x28,
            0x29,0x2A,0x34,0x35,0x36,0x37,0x38,0x39,0x3A,0x43,0x44,0x45,
            0x46,0x47,0x48,0x49,0x4A,0x53,0x54,0x55,0x56,0x57,0x58,0x59,
            0x5A,0x63,0x64,0x65,0x66,0x67,0x68,0x69,0x6A,0x73,0x74,0x75,
            0x76,0x77,0x78,0x79,0x7A,0x83,0x84,0x85,0x86,0x87,0x88,0x89,
            0x8A,0x93,0x94,0x95,0x96,0x97,0x98,0x99,0x9A,0xA2,0xA3,0xA4,
            0xA5,0xA6,0xA7,0xA8,0xA9,0xAA,0xB2,0xB3,0xB4,0xB5,0xB6,0xB7,
            0xB8,0xB9,0xBA,0xC2,0xC3,0xC4,0xC5,0xC6,0xC7,0xC8,0xC9,0xCA,
            0xD2,0xD3,0xD4,0xD5,0xD6,0xD7,0xD8,0xD9,0xDA,0xE1,0xE2,0xE3,
            0xE4,0xE5,0xE6,0xE7,0xE8,0xE9,0xEA,0xF1,0xF2,0xF3,0xF4,0xF5,
            0xF6,0xF7,0xF8,0xF9,0xFA,0xFF,0xDA,0x00,0x08,0x01,0x01,0x00,
            0x00,0x3F,0x00,0xFB,0xD6,0xFF,0xD9,
        ])


def _grab_frame(node_id="local"):
    """Return JPEG bytes from real camera or synthetic frame."""
    global _camera_cap, _anomaly_log
    with _camera_lock:
        if _camera_cap is not None:
            try:
                import cv2
                ret, frame = _camera_cap.read()
                if ret:
                    ret2, buf = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, 80])
                    if ret2:
                        return bytes(buf)
            except Exception:
                pass
        return _synthetic_jpeg(label=node_id.upper())

_init_camera()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Caesar console API and dashboard")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8090)
    parser.add_argument("--latest", default="output/caesar/latest_tracks.json")
    parser.add_argument("--journal", default="output/caesar/journal.jsonl")
    parser.add_argument("--high-interest", default="output/caesar/high_interest.jsonl")
    parser.add_argument("--regional-summary", default="output/caesar/control_plane/regional_summary.json")
    parser.add_argument("--orchestration-plan", default="output/caesar/control_plane/orchestration_plan.json")
    parser.add_argument("--learning-plan", default="output/caesar/control_plane/learning_plan.json")
    parser.add_argument("--node-registry", default="output/caesar/control_plane/node_registry.json")
    parser.add_argument("--governance-audit", default="output/caesar/control_plane/governance_audit.jsonl")
    parser.add_argument("--correlated-tracks", default="output/caesar/correlated_tracks.json",
                        help="Path to the correlated_tracks.json written by the Rust hub")
    parser.add_argument("--ast-plan-dir", default="output/caesar/control_plane",
                        help="Directory where orchestrator writes AST mission plan .json files")
    parser.add_argument("--activity-window-seconds", type=int, default=900)
    parser.add_argument("--journal-scan-limit", type=int, default=2000)
    parser.add_argument("--high-interest-scan-limit", type=int, default=2000)
    # C5 FIX: Configurable MQTT broker so dashboard commands reach the correct node
    parser.add_argument("--mqtt-broker", default="localhost",
                        help="MQTT broker hostname for actuator dispatch (default: localhost)")
    parser.add_argument("--mqtt-broker-port", type=int, default=1883,
                        help="MQTT broker port (default: 1883)")
    parser.add_argument(
        '--node-cameras',
        default='',
        help='Comma-separated NODE_ID=IP:MJPEG_PORT pairs, e.g. pi-alpha=192.168.1.10:8554,pi-bravo=192.168.1.11:8554'
    )
    return parser.parse_args()


class CaesarConsoleHandler(BaseHTTPRequestHandler):
    latest_path: Path
    journal_path: Path
    high_interest_path: Path
    regional_summary_path: Path
    orchestration_plan_path: Path
    learning_plan_path: Path
    node_registry_path: Path
    governance_audit_path: Path
    correlated_tracks_path: Path
    static_dir: Path
    activity_window_seconds: int
    journal_scan_limit: int
    high_interest_scan_limit: int
    mqtt_broker_host: str   # C5: broker host read from --mqtt-broker, not hardcoded
    mqtt_broker_port: int   #     avoids always sending commands to localhost
    # FED: base directory that contains fed_contributions/ and global_model.json
    _control_plane_dir: str

    def do_GET(self) -> None:
        parsed = urlparse(self.path)

        if parsed.path == "/healthz":
            return self.write_json({"status": "ok"})
        if parsed.path == "/api/latest":
            return self.write_json(read_latest(self.latest_path))
        if parsed.path == "/api/journal":
            limit = _safe_limit(parse_qs(parsed.query), "limit", 100)
            return self.write_json(read_jsonl_tail(self.journal_path, limit))
        if parsed.path == "/api/high-interest":
            limit = _safe_limit(parse_qs(parsed.query), "limit", 100)
            return self.write_json(read_jsonl_tail(self.high_interest_path, limit))
        if parsed.path == "/api/regional-summary":
            return self.write_json(read_json(self.regional_summary_path))
        if parsed.path == "/api/orchestration":
            return self.write_json(read_json(self.orchestration_plan_path))
        if parsed.path == "/api/learning-plan":
            return self.write_json(read_json(self.learning_plan_path))
        if parsed.path == "/api/node-registry":
            return self.write_json(read_json(self.node_registry_path))
        if parsed.path == "/api/governance-audit":
            limit = _safe_limit(parse_qs(parsed.query), "limit", 25)
            return self.write_json(read_jsonl_tail(self.governance_audit_path, limit))

        # ── FEDERATED LEARNING ENDPOINTS ─────────────────────────────────────
        if parsed.path == '/api/fed/global-model':
            gm = Path(self._control_plane_dir) / 'global_model.json'
            data = read_json(gm)  # returns {} if file absent or too large
            return self.write_json(data or {'version': 0, 'global_thresholds': {}})
        # ─────────────────────────────────────────────────────────────────────

        # ── MULTI-NODE CORRELATED TRACKS ──────────────────────────────────────
        if parsed.path == '/api/correlated-tracks':
            # Returns the cross-node merged track map written by the Rust hub.
            # Returns {} when the hub has not yet produced the file (first run,
            # or correlated_tracks_path not set in hub-dev.toml).
            data = read_json(self.correlated_tracks_path)
            return self.write_json(data or {})
        # ─────────────────────────────────────────────────────────────────────

        # ── AST (Adaptive Selective Tilling) PLAN ENDPOINT ────────────────────
        if parsed.path == '/api/ast-plan':
            # Returns the AST mission plan for a given field_id.
            # The orchestrator writes these to --ast-plan-dir/{field_id}.json.
            qs = parse_qs(parsed.query)
            field_id = qs.get('field_id', [''])[0]
            # Sanitise field_id to safe filename chars (same RE as node_id)
            if not _NODE_ID_RE.match(field_id):
                self.send_error(HTTPStatus.BAD_REQUEST, 'invalid field_id')
                return
            plan_path = Path(getattr(self, '_ast_plan_dir', 'output/caesar/control_plane')) / f'{field_id}.ast.json'
            data = read_json(plan_path)
            if not data:
                self.send_error(HTTPStatus.NOT_FOUND, f'No AST plan for field_id={field_id}')
                return
            return self.write_json(data)
        # ─────────────────────────────────────────────────────────────────────

        if parsed.path == "/api/stats":
            stats = build_stats(
                read_latest(self.latest_path),
                read_jsonl_tail(self.journal_path, self.journal_scan_limit),
                read_jsonl_tail(self.high_interest_path, self.high_interest_scan_limit),
                read_json(self.node_registry_path),
                read_json(self.learning_plan_path),
                self.activity_window_seconds,
            )
            return self.write_json(stats)

        # ── LIVE CAMERA ENDPOINTS ─────────────────────────────────────────────
        if parsed.path == "/api/camera-frame":
            node_id = _safe_node_id(parse_qs(parsed.query).get("node", ["local"])[0])
            jpeg = _grab_frame(node_id)
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "image/jpeg")
            self.send_header("Content-Length", str(len(jpeg)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            self.wfile.write(jpeg)
            return

        if parsed.path == "/api/camera-stream":
            # Multipart MJPEG stream — browsers consume this natively
            node_id = _safe_node_id(parse_qs(parsed.query).get("node", ["local"])[0])
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "multipart/x-mixed-replace; boundary=caesarframe")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            try:
                while True:
                    jpeg = _grab_frame(node_id)
                    header = (
                        f"--caesarframe\r\n"
                        f"Content-Type: image/jpeg\r\n"
                        f"Content-Length: {len(jpeg)}\r\n\r\n"
                    ).encode()
                    self.wfile.write(header + jpeg + b"\r\n")
                    self.wfile.flush()
                    time.sleep(0.08)  # ~12.5 fps
            except (BrokenPipeError, ConnectionResetError):
                pass  # client closed tab
            return

        if parsed.path == '/api/proxy-camera':
            node_id = _safe_node_id(parse_qs(parsed.query).get('node', [''])[0])
            return self._proxy_mjpeg_stream(node_id)

        if parsed.path == "/api/anomaly-log":
            limit = _safe_limit(parse_qs(parsed.query), "limit", 20)
            # Auto-populate from live high-interest file if log is sparse
            if len(_anomaly_log) < 5:
                recent = read_jsonl_tail(self.high_interest_path, 20)
                for r in recent:
                    body = r.get("envelope", {}).get("body", {})
                    if body.get("track_id") and body.get("threat_level"):
                        entry = f"[LIVE] {body['track_id']} \u00b7 {body['threat_level']} \u00b7 conf:{body.get('confidence', 0):.3f}"
                        if entry not in _anomaly_log:
                            _anomaly_log.append(entry)
            return self.write_json(_anomaly_log[-limit:][::-1])

        if parsed.path == "/api/live-events":
            # Server-Sent Events: push combined stats+latest every 2 s
            # Iteration 1: also include the newest high-interest alert if a
            # new line has been appended since the last tick.
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("X-Accel-Buffering", "no")
            self.end_headers()
            try:
                ticks = 0
                while True:
                    stats = build_stats(
                        read_latest(self.latest_path),
                        read_jsonl_tail(self.journal_path, self.journal_scan_limit),
                        read_jsonl_tail(self.high_interest_path, self.high_interest_scan_limit),
                        read_json(self.node_registry_path),
                        read_json(self.learning_plan_path),
                        self.activity_window_seconds,
                    )
                    latest = read_latest(self.latest_path)
                    payload: dict = {"stats": stats, "latest": latest}

                    # ── Alert push (Iteration 1) ───────────────────────────────
                    alert = _check_alerts(self.high_interest_path)
                    if alert:
                        payload["latest_alert"] = alert
                        payload["alert_timestamp"] = time.time()
                    # ─────────────────────────────────────────────────────────────

                    self.wfile.write(f"data: {json.dumps(payload)}\r\n\r\n".encode())

                    ticks += 1
                    if ticks % 7 == 0:
                        # 14-second heartbeat to prevent proxy dead-drops
                        self.wfile.write(b": ping\n\n")

                    self.wfile.flush()
                    time.sleep(2.0)
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
            return

        if parsed.path == "/api/alert-stream":
            # Dedicated SSE endpoint for alert-only listeners (lower overhead).
            # Pushes only when a new line is appended to high_interest.jsonl.
            # Iteration 4: handles file-not-found / OS errors without crashing.
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Connection", "keep-alive")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("X-Accel-Buffering", "no")
            self.end_headers()
            # Per-connection size cursor — independent of the module-level one
            # so multiple simultaneous listeners each see all new events.
            local_last_size: int = 0
            try:
                while True:
                    # ── Iteration 4: safe stat ────────────────────────────────
                    try:
                        sz = (
                            self.high_interest_path.stat().st_size
                            if self.high_interest_path.exists()
                            else 0
                        )
                    except OSError:
                        sz = 0
                    # ─────────────────────────────────────────────────────────────
                    if sz > local_last_size:
                        # New data appended — read all new lines
                        try:
                            with self.high_interest_path.open('r', encoding='utf-8') as fh:
                                all_lines = [l for l in fh.readlines() if l.strip()]
                            # Estimate how many bytes the old content was to find new lines.
                            # Because we only need the freshly-appended ones we re-read
                            # from the start and skip lines that were already present
                            # (identified by byte offset tracking is simpler: just send
                            # all lines whose cumulative size exceeds local_last_size).
                            new_lines = []
                            cumulative = 0
                            for line in all_lines:
                                line_bytes = len((line + "\n").encode('utf-8'))
                                if cumulative + line_bytes > local_last_size:
                                    new_lines.append(line)
                                cumulative += line_bytes
                            for raw_line in new_lines:
                                try:
                                    alert_obj = json.loads(raw_line)
                                    self.wfile.write(
                                        f"event: alert\ndata: {json.dumps(alert_obj)}\n\n".encode()
                                    )
                                except (json.JSONDecodeError, ValueError):
                                    pass  # skip malformed lines
                            self.wfile.flush()
                        except (OSError, UnicodeDecodeError):
                            pass  # file disappeared or encoding error — skip tick
                        local_last_size = sz
                    else:
                        # Send a keepalive comment so the connection stays open
                        self.wfile.write(b": keepalive\n\n")
                        self.wfile.flush()
                    time.sleep(1.0)
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass  # client disconnected
            return

        if parsed.path in {"/", "/index.html"}:
            return self.serve_static("index.html")
        if parsed.path in {"/agri", "/agri.html"}:
            return self.serve_static("agri.html")
        if parsed.path in {"/infra", "/infra.html"}:
            return self.serve_static("infra.html")
        if parsed.path.startswith("/static/"):
            return self.serve_static(parsed.path.removeprefix("/static/"))

        self.send_error(HTTPStatus.NOT_FOUND, "Not found")


    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/api/actuate":
            import os
            expected_token = os.getenv("CAESAR_API_TOKEN")
            if expected_token:
                auth_header = self.headers.get("Authorization", "")
                if not auth_header.startswith("Bearer ") or auth_header.split(" ")[1] != expected_token:
                    self.send_error(HTTPStatus.UNAUTHORIZED, "Invalid or missing CAESAR_API_TOKEN")
                    return

            # F12 FIX: Handle missing Content-Length gracefully.
            # Security: cap the read at _MAX_POST_BYTES regardless of what the
            # client advertises — prevents a crafted Content-Length from blocking
            # the handler thread on a very large rfile.read() call.
            raw_cl = self.headers.get('Content-Length')
            try:
                advertised = int(raw_cl) if raw_cl is not None else _MAX_POST_BYTES
            except (ValueError, OverflowError):
                advertised = _MAX_POST_BYTES
            content_length = max(0, min(advertised, _MAX_POST_BYTES))
            post_data = self.rfile.read(content_length)
            try:
                command = json.loads(post_data)
                if not isinstance(command, dict):
                    raise ValueError("JSON body must be an object")
                node_id = command.get("node_id", "unknown")
                print(f"[caesar.hub.actuate] Dispatching command to {node_id}: {command}")
                
                # C5 FIX: Use the configured broker host/port instead of hardcoded "localhost".
                # F13 FIX: Pass a timeout to mqtt.publish.single so a dead broker
                # cannot block this HTTP handler thread indefinitely.
                try:
                    import paho.mqtt.publish as mqtt_publish
                    mqtt_publish.single(
                        f"caesar/commands/{node_id}",
                        payload=json.dumps(command),
                        hostname=self.mqtt_broker_host,
                        port=self.mqtt_broker_port,
                        keepalive=5,
                    )
                except Exception as e:
                    # Catch both paho protocol errors and lower-level socket/OS
                    # errors.  Import lazily so the name is available without
                    # requiring a top-level import of the paho client module.
                    try:
                        from paho.mqtt.client import MQTTException
                        if isinstance(e, MQTTException):
                            print(f"[caesar.hub.actuate] MQTT protocol error (broker={self.mqtt_broker_host}:{self.mqtt_broker_port}): {e}")
                        else:
                            print(f"[caesar.hub.actuate] MQTT publish failed (broker={self.mqtt_broker_host}:{self.mqtt_broker_port}): {e}")
                    except ImportError:
                        print(f"[caesar.hub.actuate] MQTT publish failed (broker={self.mqtt_broker_host}:{self.mqtt_broker_port}): {e}")

                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json")
                self.send_header("Access-Control-Allow-Origin", "*")
                self.end_headers()
                self.wfile.write(json.dumps({"status": "dispatched", "command": command}).encode())
            except Exception as e:
                self.send_error(HTTPStatus.BAD_REQUEST, str(e))
            return

        # ── FEDERATED LEARNING ENDPOINTS ─────────────────────────────────────
        if parsed.path == '/api/fed/contribute':
            # Edge nodes POST their per-class confidence statistics here.
            # Security: cap reads at _MAX_POST_BYTES (64 KiB); validate node_id
            # against _NODE_ID_RE before writing to disk to prevent path traversal.
            try:
                raw_cl = self.headers.get('Content-Length')
                try:
                    advertised = int(raw_cl) if raw_cl is not None else _MAX_POST_BYTES
                except (ValueError, OverflowError):
                    advertised = _MAX_POST_BYTES
                length = max(0, min(advertised, _MAX_POST_BYTES))
                body = self.rfile.read(length)
                payload = json.loads(body.decode('utf-8', errors='replace'))
                node_id = payload.get('node_id', '')
                if not node_id or not isinstance(node_id, str) or not _NODE_ID_RE.match(node_id):
                    self.send_error(HTTPStatus.BAD_REQUEST, 'invalid node_id')
                    return
                contrib_dir = Path(self._control_plane_dir) / 'fed_contributions'
                contrib_dir.mkdir(parents=True, exist_ok=True)
                out = contrib_dir / f'{node_id}.json'
                # Atomic write: tmp then rename so the aggregator never reads
                # a partially-written contribution file.
                tmp = out.with_suffix('.tmp')
                tmp.write_text(body.decode('utf-8', errors='replace'), encoding='utf-8')
                tmp.replace(out)
                print(f'[caesar.fed.contribute] Accepted contribution from node_id={node_id}')
                return self.write_json({'status': 'accepted'})
            except json.JSONDecodeError as e:
                self.send_error(HTTPStatus.BAD_REQUEST, f'JSON parse error: {e}')
                return
            except Exception as e:
                self.send_error(HTTPStatus.INTERNAL_SERVER_ERROR, str(e))
                return
        # ─────────────────────────────────────────────────────────────────────
        # ─────────────────────────────────────────────────────────────────────

        # ── REMOTE RECONFIGURE ───────────────────────────────────────────────
        if parsed.path == '/api/reconfigure':
            # Publish a config patch to a specific node via MQTT.
            # Only whitelisted fields are accepted to prevent injection attacks.
            _ALLOWED_PATCH_FIELDS = frozenset({'threat_threshold', 'domain', 'fusion_window_ms', 'anomaly_threshold'})
            try:
                raw_cl = self.headers.get('Content-Length')
                try:
                    advertised = int(raw_cl) if raw_cl is not None else _MAX_POST_BYTES
                except (ValueError, OverflowError):
                    advertised = _MAX_POST_BYTES
                length = max(0, min(advertised, _MAX_POST_BYTES))
                body = json.loads(self.rfile.read(length).decode('utf-8', errors='replace'))
                node_id = body.get('node_id', '')
                if not node_id or not isinstance(node_id, str) or not _NODE_ID_RE.match(node_id):
                    self.send_error(HTTPStatus.BAD_REQUEST, 'invalid node_id')
                    return
                patch = body.get('patch', {})
                if not isinstance(patch, dict):
                    self.send_error(HTTPStatus.BAD_REQUEST, 'patch must be an object')
                    return
                sanitized = {k: v for k, v in patch.items() if k in _ALLOWED_PATCH_FIELDS}
                if not sanitized:
                    self.send_error(HTTPStatus.BAD_REQUEST,
                        f'no allowed fields. Allowed: {sorted(_ALLOWED_PATCH_FIELDS)}')
                    return
                import paho.mqtt.publish as mqtt_publish
                mqtt_payload = json.dumps({'type': 'reconfigure', 'patch': sanitized})
                mqtt_publish.single(
                    f'caesar/commands/{node_id}',
                    payload=mqtt_payload,
                    hostname=self.mqtt_broker_host,
                    port=self.mqtt_broker_port,
                    keepalive=5,
                )
                return self.write_json({'status': 'patch_sent', 'node_id': node_id, 'applied_fields': sorted(sanitized.keys())})
            except json.JSONDecodeError as exc:
                self.send_error(HTTPStatus.BAD_REQUEST, f'JSON parse error: {exc}')
            except Exception as exc:
                self.send_error(HTTPStatus.INTERNAL_SERVER_ERROR, str(exc))
            return
        # ─────────────────────────────────────────────────────────────────────

        self.send_error(HTTPStatus.NOT_FOUND, 'Not found')

    def serve_static(self, relative_path: str) -> None:
        safe_path = (self.static_dir / relative_path).resolve()
        if not str(safe_path).startswith(str(self.static_dir.resolve())) or not safe_path.exists():
            self.send_error(HTTPStatus.NOT_FOUND, "Static asset not found")
            return

        content = safe_path.read_bytes()
        content_type = mimetypes.guess_type(str(safe_path))[0] or "application/octet-stream"
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(content)))
        self.end_headers()
        self.wfile.write(content)

    def write_json(self, payload) -> None:
        body = json.dumps(payload, indent=2).encode("utf-8")
        self.send_response(HTTPStatus.OK)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _proxy_mjpeg_stream(self, node_id: str) -> None:
        """Proxy MJPEG stream from a remote edge node's mjpeg_port to the browser.
        Falls back to the local camera stream if the node is not reachable."""
        import urllib.request
        import urllib.error

        addr = getattr(self, 'node_cameras', {}).get(node_id, '')
        if addr:
            try:
                url = f'http://{addr}/'
                req = urllib.request.Request(url)
                with urllib.request.urlopen(req, timeout=5) as resp:
                    ct = resp.headers.get(
                        'Content-Type',
                        'multipart/x-mixed-replace; boundary=caesar_frame'
                    )
                    self.send_response(200)
                    self.send_header('Content-Type', ct)
                    self.send_header('Cache-Control', 'no-cache')
                    self.send_header('X-Node-ID', node_id)
                    self.end_headers()
                    try:
                        while True:
                            chunk = resp.read(16384)
                            if not chunk:
                                break
                            self.wfile.write(chunk)
                            self.wfile.flush()
                    except (BrokenPipeError, ConnectionResetError):
                        pass
                return
            except (urllib.error.URLError, OSError) as e:
                print(
                    f'[console.proxy] Edge node {node_id} unreachable ({e}), falling back to local camera',
                    file=__import__('sys').stderr,
                )
        # Fall back to local camera stream
        self._handle_camera_stream(node_id)

    def _handle_camera_stream(self, node_id: str) -> None:
        """Emit a local MJPEG stream for the given node_id."""
        self.send_response(HTTPStatus.OK)
        self.send_header('Content-Type', 'multipart/x-mixed-replace; boundary=caesarframe')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Access-Control-Allow-Origin', '*')
        self.end_headers()
        try:
            while True:
                jpeg = _grab_frame(node_id)
                header = (
                    f'--caesarframe\r\n'
                    f'Content-Type: image/jpeg\r\n'
                    f'Content-Length: {len(jpeg)}\r\n\r\n'
                ).encode()
                self.wfile.write(header + jpeg + b'\r\n')
                self.wfile.flush()
                time.sleep(0.08)  # ~12.5 fps
        except (BrokenPipeError, ConnectionResetError):
            pass


def read_latest(path: Path) -> dict:
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def read_json(path: Path):
    if not path.exists():
        return {}
    # Guard against reading a pathologically large file into memory.
    if path.stat().st_size > _MAX_FILE_BYTES:
        print(f"[caesar] WARNING: {path} exceeds _MAX_FILE_BYTES ({_MAX_FILE_BYTES} B), skipping.",
              file=__import__('sys').stderr)
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl_tail(path: Path, limit: int) -> list[dict]:
    if not path.exists():
        return []
    # Guard against reading a pathologically large file into memory.
    if path.stat().st_size > _MAX_FILE_BYTES:
        print(f"[caesar] WARNING: {path} exceeds _MAX_FILE_BYTES ({_MAX_FILE_BYTES} B), skipping.",
              file=__import__('sys').stderr)
        return []
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    return [json.loads(line) for line in lines[-limit:]][::-1]


def record_time_ms(record: dict) -> int:
    if "received_at_ms" in record:
        return int(record["received_at_ms"])
    if "timestamp_ms" in record:
        return int(record["timestamp_ms"])
    envelope = record.get("envelope", {})
    body = envelope.get("body", {})
    return int(body.get("timestamp_ms", 0))


def record_body(record: dict) -> dict:
    return record.get("envelope", {}).get("body", {})


def build_stats(
    latest: dict,
    journal_records: list[dict],
    high_interest_records: list[dict],
    node_registry: dict,
    learning_plan: dict,
    activity_window_seconds: int,
) -> dict:
    now_ms = int(time.time() * 1000)
    active_cutoff_ms = now_ms - (activity_window_seconds * 1000)
    latest_records = [
        record
        for record in latest.values()
        if record_time_ms(record) >= active_cutoff_ms and record_body(record)
    ]
    recent_journal_records = [
        record
        for record in journal_records
        if record_time_ms(record) >= active_cutoff_ms and record_body(record)
    ]
    recent_high_interest_records = [
        record
        for record in high_interest_records
        if record_time_ms(record) >= active_cutoff_ms and record_body(record)
    ]

    node_counts = Counter(
        record_body(record)["node_id"]
        for record in latest_records
    )
    threat_counts = Counter(
        record_body(record)["threat_level"]
        for record in latest_records
    )
    modality_counts = Counter(
        str(modality)
        for record in latest_records
        for modality in record_body(record).get("contributing_modalities", [])
    )
    site_counts = Counter(
        record_body(record).get("site", "unknown")
        for record in latest_records
    )

    registered_nodes = node_registry.get("nodes", [])
    registered_node_count = len(registered_nodes)
    active_node_count = len(node_counts)
    active_high_interest_track_ids = {
        record_body(record)["track_id"]
        for record in latest_records
        if record_body(record).get("threat_level") == "high-interest"
    }
    recent_high_interest_track_ids = {
        record_body(record)["track_id"]
        for record in recent_high_interest_records
        if record_body(record).get("track_id")
    }
    activity_window_minutes = max(activity_window_seconds / 60.0, 1.0)
    throughput_events_per_min = len(recent_journal_records) / activity_window_minutes
    anomaly_probability = (
        len(active_high_interest_track_ids) / len(latest_records) if latest_records else 0.0
    )
    node_health_ratio = (
        active_node_count / registered_node_count if registered_node_count else 0.0
    )
    fed_round = learning_plan.get("federated_round", {})
    federated_participant_count = len(fed_round.get("participants", []))
    fed_alignment = (
        federated_participant_count / registered_node_count if registered_node_count else 0.0
    )
    last_detection_ms = max((record_time_ms(record) for record in latest_records), default=None)

    # ── Inference latency statistics (from FusedTrack.inference_latency_ms) ────
    latencies = []
    for record in latest_records:
        body = record_body(record)
        for _modality, lat in (body.get('inference_latency_ms') or {}).items():
            if isinstance(lat, (int, float)) and lat > 0:
                latencies.append(lat)

    if latencies:
        latencies_sorted = sorted(latencies)
        avg_latency = sum(latencies_sorted) / len(latencies_sorted)
        p95_idx = max(0, int(len(latencies_sorted) * 0.95) - 1)
        p95_latency = latencies_sorted[p95_idx]
        sla_met = sum(1 for l in latencies_sorted if l < 100) / len(latencies_sorted) * 100
    else:
        avg_latency = 0.0
        p95_latency = 0.0
        sla_met = 100.0
    # ────────────────────────────────────────────────────────────────────────────

    return {
        "activity_window_seconds": activity_window_seconds,
        "active_cutoff_ms": active_cutoff_ms,
        "latest_track_count": len(latest_records),
        "high_interest_recent_count": len(recent_high_interest_track_ids),
        "active_high_interest_count": len(active_high_interest_track_ids),
        "node_counts": dict(node_counts),
        "threat_counts": dict(threat_counts),
        "modality_counts": dict(modality_counts),
        "site_counts": dict(site_counts),
        "registered_node_count": registered_node_count,
        "active_node_count": active_node_count,
        "throughput_events_per_min": round(throughput_events_per_min, 2),
        "anomaly_probability": round(anomaly_probability, 4),
        "node_health_ratio": round(node_health_ratio, 4),
        "fed_alignment": round(fed_alignment, 4),
        "federated_participant_count": federated_participant_count,
        "recent_journal_count": len(recent_journal_records),
        "last_detection_ms": last_detection_ms,
        "stale": not latest_records,
        "avg_inference_latency_ms": round(avg_latency, 1),
        "p95_inference_latency_ms": round(p95_latency, 1),
        "sla_sub100ms_pct": round(sla_met, 1),
    }


def main() -> int:
    args = parse_args()
    handler = CaesarConsoleHandler
    handler.latest_path = Path(args.latest)
    handler.journal_path = Path(args.journal)
    handler.high_interest_path = Path(args.high_interest)
    handler.regional_summary_path = Path(args.regional_summary)
    handler.orchestration_plan_path = Path(args.orchestration_plan)
    handler.learning_plan_path = Path(args.learning_plan)
    handler.node_registry_path = Path(args.node_registry)
    handler.governance_audit_path = Path(args.governance_audit)
    handler.correlated_tracks_path = Path(args.correlated_tracks)
    handler.static_dir = Path(__file__).with_name("static")
    handler.activity_window_seconds = args.activity_window_seconds
    handler.journal_scan_limit = args.journal_scan_limit
    handler.high_interest_scan_limit = args.high_interest_scan_limit
    handler.mqtt_broker_host = args.mqtt_broker
    handler.mqtt_broker_port = args.mqtt_broker_port
    handler.node_cameras = dict(  # NODE_ID -> "IP:PORT" strings
        item.split('=', 1)
        for item in args.node_cameras.split(',')
        if '=' in item
    )
    # FED: derive the control-plane dir from --governance-audit path so that
    # fed_contributions/ and global_model.json end up next to the other files.
    handler._control_plane_dir = str(Path(args.governance_audit).parent)
    # AST: directory where orchestrator drops per-field mission plans.
    handler._ast_plan_dir = args.ast_plan_dir

    server = ThreadingHTTPServer((args.host, args.port), handler)
    print(f"Caesar console listening on http://{args.host}:{args.port}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
