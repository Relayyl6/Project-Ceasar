"""
remote_infer_server.py — Project Caesar Remote Inference Server

Runs on the hub PC (10.163.194.96) alongside caesar_console/server.py.
Accepts JPEG frames from a Pi 3 running in "remote_http" inference mode,
runs the full domain-aware Ollama VLM vision pipeline, and returns an
Observation JSON. Falls back to heuristic if Ollama is unreachable.

Usage:
    python services/remote_infer_server.py --port 9090

The Pi 3 config (configs/edge-pi3.toml) must point to:
    [inference]
    mode = "remote_http"
    ollama_endpoint = "http://10.163.194.96:9090"
"""
import argparse
import base64
import hashlib
import json
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Optional


# ---------------------------------------------------------------------------
# Domain-aware Ollama prompts — identical to inference.rs ollama_vision_infer()
# ---------------------------------------------------------------------------
DOMAIN_PROMPTS = {
    "agricultural": (
        "You are an agricultural AI sensor on a precision farm node. "
        "Analyze this camera frame for crop changes, plant growth, water stress, "
        "pest damage, or irrigation anomalies. "
        "Reply with ONE short label such as: "
        "'crop-growth-detected', 'water-stress', 'pest-damage', 'flood-risk', "
        "'healthy-crop', 'dry-soil', or 'clear'. Nothing else."
    ),
    "industrial": (
        "You are an industrial monitoring AI on a critical infrastructure node. "
        "Analyze this camera frame for equipment anomalies, overheating, "
        "leaks, unauthorized personnel, or structural changes. "
        "Reply with ONE short label such as: "
        "'overheating-equipment', 'fluid-leak', 'unauthorized-person', "
        "'structural-anomaly', 'fire-risk', 'normal-operation', or 'clear'. Nothing else."
    ),
    "tactical": (
        "You are a tactical surveillance AI on a Caesar defense node. "
        "Analyze this camera frame for threats, intruders, drones, or vehicles. "
        "Reply with ONE short label such as: "
        "'armed-intruder', 'civilian-drone', 'military-drone', 'suspicious-vehicle', "
        "'person-running', 'crowd-forming', or 'clear'. Nothing else."
    ),
}
DEFAULT_PROMPT = (
    "Analyze this camera frame. Identify the most significant object or anomaly. "
    "Reply with ONE short label. If nothing notable, reply 'clear'."
)

OLLAMA_BASE = "http://localhost:11434"
OLLAMA_MODEL = "llava"
OLLAMA_TIMEOUT_S = 25


# ---------------------------------------------------------------------------
# Heuristic fallback — mirrors heuristic_optical_infer() in inference.rs
# ---------------------------------------------------------------------------
def heuristic_infer(jpeg_bytes: bytes, sequence: int, camera_id: str, timestamp_ms: int) -> dict:
    """Pure-math fallback: sample first 256 bytes of JPEG for brightness proxy."""
    sample = sum(jpeg_bytes[:256]) / max(len(jpeg_bytes[:256]), 1)
    confidence = min(0.96, max(0.30, (sample / 255.0) + 0.32))
    digest = hashlib.blake3(jpeg_bytes).hexdigest() if hasattr(hashlib, "blake3") else hashlib.sha256(jpeg_bytes).hexdigest()
    return {
        "track_hint": f"heuristic-track-{sequence % 6}",
        "confidence": round(confidence, 4),
        "class_label": "vehicle",
        "position_m": [32.0 + (sequence % 14), 11.0 + (sequence % 8)],
        "velocity_mps": 4.0 + (sequence % 4),
        "evidence_digest": digest,
        "inference_stage": "heuristic",
    }


# ---------------------------------------------------------------------------
# Ollama vision call — same logic as ollama_vision_infer() in inference.rs
# ---------------------------------------------------------------------------
def ollama_vision_infer(
    jpeg_bytes: bytes,
    domain: str,
    sequence: int,
    camera_id: str,
    timestamp_ms: int,
    ollama_base: str = OLLAMA_BASE,
    model: str = OLLAMA_MODEL,
) -> Optional[dict]:
    """Call Ollama /api/generate with the JPEG image. Returns None on failure."""
    prompt = DOMAIN_PROMPTS.get(domain, DEFAULT_PROMPT)
    b64_img = base64.b64encode(jpeg_bytes).decode("ascii")

    payload = json.dumps({
        "model": model,
        "prompt": prompt,
        "images": [b64_img],
        "stream": False,
    }).encode("utf-8")

    try:
        req = urllib.request.Request(
            f"{ollama_base}/api/generate",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=OLLAMA_TIMEOUT_S) as resp:
            resp_json = json.loads(resp.read().decode("utf-8"))
    except (urllib.error.URLError, OSError) as exc:
        print(f"[remote_infer] Ollama unreachable: {exc}")
        return None
    except Exception as exc:
        print(f"[remote_infer] Ollama error: {exc}")
        return None

    label = resp_json.get("response", "unknown-anomaly").strip().lower()
    eval_count = resp_json.get("eval_count", 5)
    # Shorter, more direct answers score higher confidence — same formula as Rust
    confidence = max(0.60, min(0.84, 0.84 - (max(0, eval_count - 3) * 0.012)))

    digest = hashlib.sha256(jpeg_bytes).hexdigest()
    label_short = label[:12]

    return {
        "track_hint": f"ollama-track-{sequence % 100}",
        "confidence": round(confidence, 4),
        "class_label": label,
        "position_m": [20.0 + (sequence % 20), 10.0 + (sequence % 12)],
        "velocity_mps": 0.0,
        "evidence_digest": f"ollama-{label_short}-seq{sequence}",
        "inference_stage": "ollama",
    }


# ---------------------------------------------------------------------------
# HTTP request handler
# ---------------------------------------------------------------------------
class InferHandler(BaseHTTPRequestHandler):
    """Two endpoints:
      POST /infer          — vision inference (optical JPEG from Pi 3)
      POST /api/generate   — Ollama text proxy (thermal/radar prompts from Pi 3)
      GET  /health         — health check
    """

    def log_message(self, fmt, *args):  # Quieter access log
        print(f"[remote_infer] {self.address_string()} — {fmt % args}")

    def do_POST(self):
        if self.path == "/infer":
            self._handle_infer()
        elif self.path == "/api/generate":
            self._handle_ollama_proxy()
        else:
            self._send(404, {"error": "not found"})

    def _handle_infer(self):
        """Vision inference: accepts JPEG frame, runs Ollama VLM, returns Observation JSON."""
        length = int(self.headers.get("Content-Length", 0))
        if length == 0:
            self._send(400, {"error": "empty body"})
            return

        try:
            body = json.loads(self.rfile.read(length).decode("utf-8"))
        except json.JSONDecodeError as exc:
            self._send(400, {"error": f"JSON parse error: {exc}"})
            return

        jpeg_b64   = body.get("jpeg_b64", "")
        node_id    = body.get("node_id", "unknown")
        domain     = body.get("domain", "general")
        sequence   = int(body.get("sequence", 0))
        timestamp  = int(body.get("timestamp_ms", int(time.time() * 1000)))
        camera_id  = body.get("camera_id", "camera")

        if not jpeg_b64:
            self._send(400, {"error": "jpeg_b64 field is required"})
            return

        try:
            jpeg_bytes = base64.b64decode(jpeg_b64)
        except Exception as exc:
            self._send(400, {"error": f"base64 decode failed: {exc}"})
            return

        print(
            f"[remote_infer] node={node_id} domain={domain} "
            f"seq={sequence} jpeg_size={len(jpeg_bytes)}B"
        )

        # --- 1. Try Ollama VLM ---
        result = ollama_vision_infer(
            jpeg_bytes, domain, sequence, camera_id, timestamp
        )

        # --- 2. Fallback to heuristic if Ollama fails ---
        if result is None:
            print(f"[remote_infer] Falling back to heuristic for seq={sequence}")
            result = heuristic_infer(jpeg_bytes, sequence, camera_id, timestamp)

        self._send(200, result)

    def _handle_ollama_proxy(self):
        """Transparent Ollama proxy for thermal/radar text prompts.
        
        thermal_infer() and radar_infer() in inference.rs call
        {ollama_endpoint}/api/generate with text-only prompts.
        With edge-pi3.toml pointing ollama_endpoint to port 9090,
        this proxy forwards those calls to the local Ollama instance
        at localhost:11434, so the Pi 3 only needs one hub address.
        """
        length = int(self.headers.get("Content-Length", 0))
        if length == 0:
            self._send(400, {"error": "empty body"})
            return

        raw_body = self.rfile.read(length)
        local_ollama = f"{OLLAMA_BASE}/api/generate"
        print(f"[remote_infer.proxy] Forwarding /api/generate → {local_ollama}")

        try:
            req = urllib.request.Request(
                local_ollama,
                data=raw_body,
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=OLLAMA_TIMEOUT_S) as resp:
                resp_body = resp.read()
                resp_code = resp.status
        except urllib.error.HTTPError as exc:
            resp_body = exc.read()
            resp_code = exc.code
            print(f"[remote_infer.proxy] Ollama HTTP error {resp_code}")
        except Exception as exc:
            print(f"[remote_infer.proxy] Ollama unreachable: {exc}")
            # Return a minimal valid Ollama-format response so the Rust
            # caller can parse it and use heuristic confidence.
            fallback = json.dumps({
                "response": "clear",
                "eval_count": 1,
                "done": True,
            }).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(fallback)))
            self.end_headers()
            self.wfile.write(fallback)
            return

        self.send_response(resp_code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(resp_body)))
        self.end_headers()
        self.wfile.write(resp_body)

    def do_GET(self):
        """Health check endpoint."""
        if self.path == "/health":
            self._send(200, {"status": "ok", "service": "caesar-remote-infer"})
        else:
            self._send(404, {"error": "not found"})

    def _send(self, code: int, payload: dict):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)



# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------
def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Project Caesar — Remote Inference Server (hub-side)"
    )
    parser.add_argument("--host", default="0.0.0.0", help="Bind address")
    parser.add_argument("--port", type=int, default=9090, help="Listen port")
    parser.add_argument(
        "--ollama-base",
        default=OLLAMA_BASE,
        help=f"Ollama base URL (default: {OLLAMA_BASE})",
    )
    parser.add_argument(
        "--ollama-model",
        default=OLLAMA_MODEL,
        help=f"Ollama vision model (default: {OLLAMA_MODEL})",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    global OLLAMA_BASE, OLLAMA_MODEL
    OLLAMA_BASE = args.ollama_base
    OLLAMA_MODEL = args.ollama_model

    # C2 FIX: ThreadingHTTPServer handles each request in a separate thread.
    # This prevents Ollama VLM calls (up to 25s) from blocking concurrent frame submissions.
    server = ThreadingHTTPServer((args.host, args.port), InferHandler)
    print(
        f"[remote_infer] Caesar Remote Inference Server running on "
        f"http://{args.host}:{args.port}"
    )
    print(f"[remote_infer] Ollama backend: {OLLAMA_BASE} (model: {OLLAMA_MODEL})")
    print(f"[remote_infer] POST /infer  — vision inference endpoint")
    print(f"[remote_infer] GET  /health — health check endpoint")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n[remote_infer] Shutting down.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
