import argparse
import json
import mimetypes
import time
from collections import Counter
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


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

        if parsed.path == "/":
            return self.serve_static("index.html")
        if parsed.path.startswith("/static/"):
            return self.serve_static(parsed.path.removeprefix("/static/"))

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
