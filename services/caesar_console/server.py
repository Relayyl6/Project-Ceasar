# ── AUTO-DEPENDENCY BOOTSTRAP ─────────────────────────────────────────────────
# Runs before any imports that might be missing. Ensures the server is self-
# healing: first run on a fresh machine will install what it needs, then boot.
import sys, subprocess

def _ensure(pkg: str, import_name: str | None = None) -> None:
    name = import_name or pkg
    try:
        __import__(name)
    except ModuleNotFoundError:
        print(f"[caesar.boot] Installing missing dependency: {pkg} ...")
        subprocess.check_call([sys.executable, "-m", "pip", "install", pkg, "--quiet"])
        print(f"[caesar.boot] {pkg} installed successfully.")

_ensure("opencv-python", "cv2")
_ensure("pillow",        "PIL")
_ensure("paho-mqtt",     "paho.mqtt.client")
# ──────────────────────────────────────────────────────────────────────────────

import argparse
import json
import mimetypes
import time
import struct
import math
import threading
from collections import Counter
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

# ── CAMERA STATE ─────────────────────────────────────────────────────────────
_camera_lock = threading.Lock()
_camera_cap = None          # OpenCV VideoCapture, if available
_camera_frame_bytes = None  # last JPEG bytes
_camera_node_id = "local"
_anomaly_log = []           # recent detection strings

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
    parser.add_argument("--activity-window-seconds", type=int, default=900)
    parser.add_argument("--journal-scan-limit", type=int, default=2000)
    parser.add_argument("--high-interest-scan-limit", type=int, default=2000)
    # C5 FIX: Configurable MQTT broker so dashboard commands reach the correct node
    parser.add_argument("--mqtt-broker", default="localhost",
                        help="MQTT broker hostname for actuator dispatch (default: localhost)")
    parser.add_argument("--mqtt-broker-port", type=int, default=1883,
                        help="MQTT broker port (default: 1883)")
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
    static_dir: Path
    activity_window_seconds: int
    journal_scan_limit: int
    high_interest_scan_limit: int
    mqtt_broker_host: str   # C5: broker host read from --mqtt-broker, not hardcoded
    mqtt_broker_port: int   #     avoids always sending commands to localhost

    def do_GET(self) -> None:
        parsed = urlparse(self.path)

        if parsed.path == "/healthz":
            return self.write_json({"status": "ok"})
        if parsed.path == "/api/latest":
            return self.write_json(read_latest(self.latest_path))
        if parsed.path == "/api/journal":
            limit = int(parse_qs(parsed.query).get("limit", ["100"])[0])
            return self.write_json(read_jsonl_tail(self.journal_path, limit))
        if parsed.path == "/api/high-interest":
            limit = int(parse_qs(parsed.query).get("limit", ["100"])[0])
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
            limit = int(parse_qs(parsed.query).get("limit", ["25"])[0])
            return self.write_json(read_jsonl_tail(self.governance_audit_path, limit))
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
            node_id = parse_qs(parsed.query).get("node", ["local"])[0]
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
            node_id = parse_qs(parsed.query).get("node", ["local"])[0]
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

        if parsed.path == "/api/anomaly-log":
            limit = int(parse_qs(parsed.query).get("limit", ["20"])[0])
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
                    payload = json.dumps({"stats": stats, "latest": latest})
                    self.wfile.write(f"data: {payload}\r\n\r\n".encode())
                    
                    ticks += 1
                    if ticks % 7 == 0:
                        # 14-second heartbeat to prevent proxy dead-drops
                        self.wfile.write(b": ping\n\n")
                        
                    self.wfile.flush()
                    time.sleep(2.0)
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
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
            content_length = int(self.headers.get('Content-Length', 0))
            post_data = self.rfile.read(content_length)
            try:
                command = json.loads(post_data)
                node_id = command.get("node_id", "unknown")
                print(f"[caesar.hub.actuate] Dispatching command to {node_id}: {command}")
                
                # C5 FIX: Use the configured broker host/port instead of hardcoded "localhost".
                try:
                    import paho.mqtt.publish as mqtt_publish
                    mqtt_publish.single(
                        f"caesar/commands/{node_id}",
                        payload=json.dumps(command),
                        hostname=self.mqtt_broker_host,
                        port=self.mqtt_broker_port,
                    )
                except Exception as e:
                    print(f"[caesar.hub.actuate] MQTT publish failed (broker={self.mqtt_broker_host}:{self.mqtt_broker_port}): {e}")

                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json")
                self.send_header("Access-Control-Allow-Origin", "*")
                self.end_headers()
                self.wfile.write(json.dumps({"status": "dispatched", "command": command}).encode())
            except Exception as e:
                self.send_error(HTTPStatus.BAD_REQUEST, str(e))
            return
            
        self.send_error(HTTPStatus.NOT_FOUND, "Not found")

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


def read_latest(path: Path) -> dict:
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def read_json(path: Path):
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl_tail(path: Path, limit: int) -> list[dict]:
    if not path.exists():
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
    handler.static_dir = Path(__file__).with_name("static")
    handler.activity_window_seconds = args.activity_window_seconds
    handler.journal_scan_limit = args.journal_scan_limit
    handler.high_interest_scan_limit = args.high_interest_scan_limit
    handler.mqtt_broker_host = args.mqtt_broker
    handler.mqtt_broker_port = args.mqtt_broker_port

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
