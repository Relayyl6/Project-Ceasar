import argparse
import json
import time
from collections import Counter, defaultdict
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Caesar orchestration and learning fabric service")
    parser.add_argument("--cluster-config", default="configs/mesh-cluster.json")
    parser.add_argument("--latest", default="output/caesar/latest_tracks.json")
    parser.add_argument("--high-interest", default="output/caesar/high_interest.jsonl")
    parser.add_argument("--output-dir", default="output/caesar/control_plane")
    parser.add_argument("--interval", type=int, default=15)
    parser.add_argument("--run-once", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    cluster = json.loads(Path(args.cluster_config).read_text(encoding="utf-8"))
    latest_path = Path(args.latest)
    high_interest_path = Path(args.high_interest)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    while True:
        latest = read_latest(latest_path)
        alerts = read_jsonl_tail(high_interest_path, 200)

        node_registry = build_node_registry(cluster, latest)
        regional_summary = build_regional_summary(cluster, latest, alerts)
        orchestration_plan = build_orchestration_plan(cluster, latest, alerts)
        learning_plan = build_learning_plan(cluster, latest, alerts)

        write_json(output_dir / "node_registry.json", node_registry)
        write_json(output_dir / "regional_summary.json", regional_summary)
        write_json(output_dir / "orchestration_plan.json", orchestration_plan)
        write_json(output_dir / "learning_plan.json", learning_plan)
        append_jsonl(
            output_dir / "governance_audit.jsonl",
            {
                "timestamp_ms": int(time.time() * 1000),
                "cluster_id": cluster["cluster_id"],
                "regional_summary": {
                    "active_nodes": regional_summary["active_node_count"],
                    "active_tracks": regional_summary["active_track_count"],
                    "dominant_threat_level": regional_summary["dominant_threat_level"],
                },
                "policy_digest": orchestration_plan["policy_digest"],
                "federated_round": learning_plan["federated_round"]["round_id"],
            },
        )

        if args.run_once:
            break
        time.sleep(args.interval)

    return 0


def read_latest(path: Path) -> dict:
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl_tail(path: Path, limit: int) -> list[dict]:
    if not path.exists():
        return []
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    return [json.loads(line) for line in lines[-limit:]]


def build_node_registry(cluster: dict, latest: dict) -> dict:
    latest_records = list(latest.values())
    active_nodes = {
        record["envelope"]["body"]["node_id"]: record["received_at_ms"]
        for record in latest_records
        if "envelope" in record and "body" in record["envelope"]
    }

    registry = []
    for node in cluster["nodes"]:
        registry.append(
            {
                **node,
                "active": node["node_id"] in active_nodes,
                "last_seen_ms": active_nodes.get(node["node_id"]),
            }
        )
    return {"cluster_id": cluster["cluster_id"], "nodes": registry}


def build_regional_summary(cluster: dict, latest: dict, alerts: list[dict]) -> dict:
    latest_records = list(latest.values())
    threat_counts = Counter()
    modality_counts = Counter()
    node_counts = Counter()
    site_counts = Counter()

    for record in latest_records:
        body = record["envelope"]["body"]
        threat_counts[body["threat_level"]] += 1
        node_counts[body["node_id"]] += 1
        site_counts[body["site"]] += 1
        for modality in body["contributing_modalities"]:
            modality_counts[modality] += 1

    dominant_threat = threat_counts.most_common(1)[0][0] if threat_counts else "none"

    return {
        "cluster_id": cluster["cluster_id"],
        "region": cluster["region"],
        "active_node_count": len(node_counts),
        "active_track_count": len(latest_records),
        "high_interest_recent_count": len(alerts),
        "dominant_threat_level": dominant_threat,
        "threat_counts": dict(threat_counts),
        "modality_counts": dict(modality_counts),
        "site_activity": dict(site_counts),
    }


def build_orchestration_plan(cluster: dict, latest: dict, alerts: list[dict]) -> dict:
    latest_records = list(latest.values())
    per_node_high_interest = Counter(
        record["envelope"]["body"]["node_id"]
        for record in alerts
        if "envelope" in record and "body" in record["envelope"]
    )

    routing_actions = []
    relay_actions = []
    for node in cluster["nodes"]:
        if node["role"] == "fixed_tower":
            priority = "high" if per_node_high_interest[node["node_id"]] else "normal"
            preferred_protocol = "dds" if priority == "high" else "mqtt"
            routing_actions.append(
                {
                    "node_id": node["node_id"],
                    "priority": priority,
                    "preferred_protocol": preferred_protocol,
                    "secondary_protocol": "zenoh",
                }
            )
        if node["role"] == "relay":
            relay_actions.append(
                {
                    "node_id": node["node_id"],
                    "assignment": "mesh-heal",
                    "target_zone": highest_pressure_zone(latest_records),
                }
            )

    return {
        "cluster_id": cluster["cluster_id"],
        "policy_digest": {
            "high_priority_protocol": "dds",
            "low_bandwidth_protocol": "mqtt",
            "mesh_discovery_protocol": "zenoh",
            "regional_exchange_protocol": "amqp",
        },
        "routing_actions": routing_actions,
        "relay_actions": relay_actions,
    }


def build_learning_plan(cluster: dict, latest: dict, alerts: list[dict]) -> dict:
    latest_records = list(latest.values())
    node_tracks = defaultdict(list)
    for record in latest_records:
        body = record["envelope"]["body"]
        node_tracks[body["node_id"]].append(body)

    supervised_jobs = []
    semi_supervised_jobs = []
    reinforcement_jobs = []

    for node in cluster["nodes"]:
        tracks = node_tracks[node["node_id"]]
        if "sl" in node["learning_layers"]:
            supervised_jobs.append(
                {
                    "node_id": node["node_id"],
                    "job_type": "supervised_recalibration",
                    "target_model": "detector-head",
                    "label_budget": max(10, len(tracks) * 2),
                    "trigger": "regional-threat-drift",
                }
            )
        if "usl" in node["learning_layers"]:
            semi_supervised_jobs.append(
                {
                    "node_id": node["node_id"],
                    "job_type": "anomaly_autoencoder_refresh",
                    "target_model": "environmental-anomaly-detector",
                    "window_size": max(50, len(tracks) * 5),
                    "trigger": "confidence-spread-shift",
                }
            )
        if "rl" in node["learning_layers"]:
            reinforcement_jobs.append(
                {
                    "node_id": node["node_id"],
                    "job_type": "routing_policy_update",
                    "target_policy": "mesh-traffic-coordinator",
                    "reward_signal": "alert_delivery_latency_vs_bandwidth",
                    "trigger": "relay-load-change",
                }
            )

    participants = [
        node["node_id"]
        for node in cluster["nodes"]
        if node["role"] in {"fixed_tower", "regional_hub"}
    ]

    return {
        "cluster_id": cluster["cluster_id"],
        "supervised_learning": supervised_jobs,
        "semi_supervised_learning": semi_supervised_jobs,
        "reinforcement_learning": reinforcement_jobs,
        "federated_round": {
            "round_id": int(time.time()),
            "strategy": cluster["federated_strategy"],
            "participants": participants,
            "aggregation_target": "regional-hub",
            "global_models": [
                "detector-head",
                "environmental-anomaly-detector",
                "mesh-traffic-coordinator",
            ],
        },
    }


def highest_pressure_zone(records: list[dict]) -> str:
    if not records:
        return "idle"
    zone_counts = Counter(record["envelope"]["body"]["site"] for record in records)
    return zone_counts.most_common(1)[0][0]


def write_json(path: Path, payload: dict) -> None:
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def append_jsonl(path: Path, payload: dict) -> None:
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(payload) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
