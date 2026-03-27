import argparse
import json
import mimetypes
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
    return parser.parse_args()


class CaesarConsoleHandler(BaseHTTPRequestHandler):
    latest_path: Path
    journal_path: Path
    high_interest_path: Path
    regional_summary_path: Path
    orchestration_plan_path: Path
    learning_plan_path: Path
    node_registry_path: Path
    static_dir: Path

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
        if parsed.path == "/api/stats":
            stats = build_stats(
                read_latest(self.latest_path),
                read_jsonl_tail(self.high_interest_path, 250),
                read_json(self.node_registry_path),
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


def build_stats(latest: dict, high_interest_records: list[dict], node_registry: dict) -> dict:
    latest_records = list(latest.values())
    node_counts = Counter(
        record["envelope"]["body"]["node_id"]
        for record in latest_records
        if "envelope" in record and "body" in record["envelope"]
    )
    threat_counts = Counter(
        record["envelope"]["body"]["threat_level"]
        for record in latest_records
        if "envelope" in record and "body" in record["envelope"]
    )

    return {
        "latest_track_count": len(latest_records),
        "high_interest_recent_count": len(high_interest_records),
        "node_counts": dict(node_counts),
        "threat_counts": dict(threat_counts),
        "registered_node_count": len(node_registry.get("nodes", [])),
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
    handler.static_dir = Path(__file__).with_name("static")

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
