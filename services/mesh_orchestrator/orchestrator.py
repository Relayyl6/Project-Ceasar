import argparse
import json
import math
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path
import asyncio
import zmq
import zmq.asyncio
import uuid
import hashlib

def get_offset_waypoint(base_pos, drone_id: str, offset_meters: float = 15.0):
    """
    Generate an offset waypoint based on the drone ID to prevent mid-air collisions.
    Uses a hash of the drone_id to pick a unique approach angle.
    """
    h = int(hashlib.md5(drone_id.encode('utf-8')).hexdigest(), 16)
    angle = (h % 360) * (math.pi / 180.0)
    
    lat, lon = base_pos
    lat_offset = (offset_meters * math.cos(angle)) / 111320.0
    lon_offset = (offset_meters * math.sin(angle)) / (111320.0 * math.cos(math.radians(lat)))
    
    return (lat + lat_offset, lon + lon_offset)
# Project Caesar schema validations and tracker
from schemas import ControlPayload, DroneDispatchCommand, PTZCommand
from tracker import MultiCameraTracker
from alerts import AgenticAlerter

# ── Iteration 3: ensure ast_planner is importable regardless of CWD ──────────
# The orchestrator may be launched from any working directory.  We insert the
# directory that contains this file so that `import ast_planner` always resolves
# to the sibling module in services/mesh_orchestrator/.
_THIS_DIR = Path(__file__).resolve().parent
if str(_THIS_DIR) not in sys.path:
    sys.path.insert(0, str(_THIS_DIR))

import ast_planner as astp
from fed_aggregator import FedAggregator
# ─────────────────────────────────────────────────────────────────────────────

import os
import paho.mqtt.client as _mqtt
from threat_engine import ThreatClassifier, AutonomousResponseEngine
from route_predictor import predict_route
from pydantic import BaseModel, field_validator, ValidationError
import re as _re

class _IncomingTrack(BaseModel):
    track_id: str
    node_id: str
    threat_level: str
    confidence: float
    class_label: str
    timestamp_ms: int
    geo_latitude: float = 0.0
    geo_longitude: float = 0.0

    @field_validator("confidence")
    @classmethod
    def _conf_range(cls, v):
        if not 0.0 <= v <= 1.0:
            raise ValueError(f"confidence {v} out of [0,1]")
        return v

    @field_validator("node_id", "track_id")
    @classmethod
    def _safe_id(cls, v):
        if not _re.match(r"^[\w.\-]{1,128}$", v):
            raise ValueError(f"unsafe id: {v!r}")
        return v

    @field_validator("threat_level")
    @classmethod
    def _valid_level(cls, v):
        allowed = {"monitor", "high-interest", "low", "medium", "high", "critical"}
        if v not in allowed:
            raise ValueError(f"unknown threat_level: {v!r}")
        return v

def _dispatch_drone_mqtt(cmd_dict: dict, broker: str, port: int) -> bool:
    """Publish a DroneDispatchCommand to MQTT caesar/commands/{drone_id}."""
    drone_id = cmd_dict.get("drone_id", "unknown")
    topic = f"caesar/commands/{drone_id}"
    try:
        payload = json.dumps(cmd_dict).encode("utf-8")
        client = _mqtt.Client(client_id=f"caesar-orc-{os.getpid()}")
        client.connect(broker, port, keepalive=10)
        rc, _ = client.publish(topic, payload, qos=1)
        client.disconnect()
        if rc == _mqtt.MQTT_ERR_SUCCESS:
            print(f"[orchestrator.mqtt] Drone command dispatched via MQTT → {topic}")
            return True
        print(f"[orchestrator.mqtt] MQTT publish failed rc={rc}")
    except Exception as exc:
        print(f"[orchestrator.mqtt] Drone MQTT dispatch error: {exc}")
    return False

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Caesar orchestration and learning fabric service")
    parser.add_argument("--cluster-config", default="configs/mesh-cluster.json")
    parser.add_argument("--latest", default="output/caesar/latest_tracks.json")
    parser.add_argument("--high-interest", default="output/caesar/high_interest.jsonl")
    parser.add_argument("--output-dir", default="output/caesar/control_plane")
    parser.add_argument("--interval", type=int, default=15)
    parser.add_argument("--run-once", action="store_true")
    parser.add_argument("--mqtt-broker", default="127.0.0.1", help="MQTT broker host")
    parser.add_argument("--mqtt-port", type=int, default=1883, help="MQTT broker port")
    return parser.parse_args()


async def threat_escalation_loop(args, cluster):
    ctx = zmq.asyncio.Context()
    sub = ctx.socket(zmq.SUB)
    sub.connect("tcp://127.0.0.1:5555")
    sub.setsockopt_string(zmq.SUBSCRIBE, "")
    
    tracker = MultiCameraTracker(max_age=10, min_hits=1, distance_threshold=0.01) # threshold in degrees lat/lon roughly ~1km
    alerter = AgenticAlerter()
    print("[orchestrator.escalation] Real-time tracking engine listening on tcp://127.0.0.1:5555")
    
    loop = asyncio.get_event_loop()
    
    while True:
        try:
            raw = await sub.recv_string()
            msg = json.loads(raw)
            try:
                validated = _IncomingTrack(**msg)
            except ValidationError as e:
                print(f"[orchestrator.escalation] Dropped invalid ZMQ track ({e.error_count()} errors)")
                continue
        except json.JSONDecodeError as e:
            print(f"[orchestrator.escalation] Dropped malformed ZMQ JSON: {e}")
            continue
        except Exception as e:
            print(f"[orchestrator.escalation] ZMQ recv error: {e}")
            continue
            
        lat, lon = validated.geo_latitude, validated.geo_longitude
        if lat == 0 and lon == 0:
            continue
            
        det = {
            'id': validated.track_id,
            'pos': (lat, lon), 
            'threat': validated.threat_level,
            'confidence': validated.confidence,
            'class_label': validated.class_label,
            'modality': 'optical'
        }
        confirmed_tracks = tracker.update([det])
        
        for trk in confirmed_tracks:
            # ── Stream 2: Threat Escalation & Route Prediction ──
            trk['threat_level'] = ThreatClassifier.evaluate(trk.get('observations', []), trk['threat_level'])
            trk['predicted_route'] = predict_route(trk['pos'], trk['vel'], [5.0, 15.0, 30.0])

            if trk['threat_level'] in ['critical', 'high-interest', 'high']:
                if trk['threat_level'] == 'critical':
                    # Dispatch Agentic Call to authorities, offloaded to thread pool (DS-3)
                    asyncio.create_task(
                        asyncio.to_thread(
                            alerter.trigger_authority_call,
                            "Critical Armed Threat" if "armed" in validated.class_label else "Critical Threat",
                            trk['pos'][0],
                            trk['pos'][1]
                        )
                    )
                    
                available_drones = [n["node_id"] for n in cluster.get("nodes", []) if n.get("role") == "drone"]
                if not available_drones:
                    available_drones = ["drone-alpha"]
                
                responses = AutonomousResponseEngine.generate_response(trk, [], available_drones)

                for d_cmd in responses['drone_commands']:
                    drone_dispatch = DroneDispatchCommand(
                        command_type="dispatch-drone",
                        drone_id=d_cmd['drone_id'],
                        target_zone=d_cmd['target_zone'],
                        waypoints=d_cmd['waypoints'],
                        priority=10
                    )
                    
                    payload = ControlPayload(
                        payload_id=str(uuid.uuid4()),
                        timestamp_ms=int(time.time()*1000),
                        threat_level=trk['threat_level'],
                        drone_commands=[drone_dispatch],
                        ptz_commands=[]
                    )
                    
                    cmd_dict = payload.model_dump()
                    cmd_dict["drone_id"] = d_cmd['drone_id'] 
                    
                    # Offload blocking MQTT dispatch to thread pool (DS-3)
                    asyncio.create_task(
                        asyncio.to_thread(_dispatch_drone_mqtt, cmd_dict, args.mqtt_broker, args.mqtt_port)
                    )

async def periodic_orchestration_loop(args, cluster, fed, latest_path, high_interest_path, output_dir):
    while True:
        try:
            # Offload blocking file IO to thread pool (B5)
            latest = await asyncio.to_thread(read_latest, latest_path)
            alerts = await asyncio.to_thread(read_jsonl_tail, high_interest_path, 200)

            node_registry = build_node_registry(cluster, latest)
            regional_summary = build_regional_summary(cluster, latest, alerts)
            orchestration_plan = build_orchestration_plan(cluster, latest, alerts)
            learning_plan = build_learning_plan(cluster, latest, alerts)

            await asyncio.to_thread(write_json, output_dir / "node_registry.json", node_registry)
            await asyncio.to_thread(write_json, output_dir / "regional_summary.json", regional_summary)
            await asyncio.to_thread(write_json, output_dir / "orchestration_plan.json", orchestration_plan)
            await asyncio.to_thread(write_json, output_dir / "learning_plan.json", learning_plan)

            # ── FedAvg aggregation ─────────────────────────────────────────
            fed_result = fed.aggregate()
            fed_n_contributors = fed_result.get("n_contributors", 0) if fed_result else 0
            fed_n_classes = len(fed_result.get("global_thresholds", {})) if fed_result else 0
            if fed_result:
                print(
                    f"[orchestrator.fed] FedAvg complete — "
                    f"{fed_n_contributors} contributor(s), "
                    f"{fed_n_classes} class threshold(s) updated."
                )

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
                    "fed_aggregation": {
                        "n_contributors": fed_n_contributors,
                        "n_classes_updated": fed_n_classes,
                    },
                },
            )
        except Exception as e:
            print(f"Unhandled exception in orchestrator loop: {e}")

        if args.run_once:
            break
        await asyncio.sleep(args.interval)

async def async_main():
    args = parse_args()
    cluster = json.loads(Path(args.cluster_config).read_text(encoding="utf-8"))
    latest_path = Path(args.latest)
    high_interest_path = Path(args.high_interest)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    fed = FedAggregator(
        contributions_dir=output_dir / "fed_contributions",
        global_model_path=output_dir / "global_model.json",
    )

    t1 = asyncio.create_task(threat_escalation_loop(args, cluster))
    t2 = asyncio.create_task(periodic_orchestration_loop(args, cluster, fed, latest_path, high_interest_path, output_dir))
    
    if args.run_once:
        await t2
        t1.cancel()
    else:
        await asyncio.gather(t1, t2)

def main() -> int:
    try:
        asyncio.run(async_main())
    except KeyboardInterrupt:
        pass
    return 0


def read_latest(path: Path) -> dict:
    try:
        if not path.exists():
            return {}
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as e:
        print(f"Error reading latest: {e}")
        return {}


def read_jsonl_tail(path: Path, limit: int) -> list[dict]:
    try:
        if not path.exists():
            return []
        lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
        result = []
        for line in lines[-limit:]:
            try:
                result.append(json.loads(line))
            except Exception:
                pass
        return result
    except Exception as e:
        print(f"Error reading jsonl tail: {e}")
        return []


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
        if "envelope" not in record or "body" not in record["envelope"]:
            continue
        body = record["envelope"]["body"]
        threat_counts[body.get("threat_level", "unknown")] += 1
        node_counts[body.get("node_id", "unknown")] += 1
        site_counts[body.get("site", "unknown")] += 1
        for modality in body.get("contributing_modalities", []):
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
        if node["role"] == "relay" or node["role"] == "drone":
            relay_action: dict = {
                "node_id": node["node_id"],
                "assignment": "mesh-heal",
                "target_zone": highest_pressure_zone(latest_records),
                "ast_directive": "Adaptive Selective Tilling (AST) - Execute Grid Survey for 3D Terrain & NDVI mapping",
            }
            # ── Iteration 1: real AST plan for agricultural domain ─────────────
            domain = cluster.get("domain", "general")
            active_nodes = latest_records  # already-filtered list of records
            if domain == "agricultural" and active_nodes:
                # Use bounding box of all nodes as the field boundary
                lats = [
                    v.get("envelope", {}).get("body", {}).get("geo_latitude", 0)
                    for v in active_nodes
                    if v.get("envelope", {}).get("body", {}).get("geo_latitude", 0) != 0
                ]
                lons = [
                    v.get("envelope", {}).get("body", {}).get("geo_longitude", 0)
                    for v in active_nodes
                    if v.get("envelope", {}).get("body", {}).get("geo_longitude", 0) != 0
                ]
                if lats and lons:
                    planner = astp.AstPlanner(grid_spacing_m=50.0, survey_alt_m=30.0)
                    # Simple bounding box in metres
                    dlat_m = (max(lats) - min(lats)) * 111320.0
                    dlon_m = (
                        (max(lons) - min(lons))
                        * 111320.0
                        * math.cos(math.radians(sum(lats) / len(lats)))
                    )
                    # Extract body dicts for track_observations
                    track_obs = [
                        v.get("envelope", {}).get("body", {})
                        for v in active_nodes
                        if v.get("envelope", {}).get("body")
                    ]
                    plan = planner.build_grid_plan(
                        origin_lat=min(lats),
                        origin_lon=min(lons),
                        width_m=max(100.0, dlon_m),
                        height_m=max(100.0, dlat_m),
                        field_id="auto-field-1",
                        track_observations=track_obs,
                    )
                    from dataclasses import asdict
                    relay_action["ast_plan"] = asdict(plan)
                    relay_action["ast_directive"] = (
                        f"AST Grid Survey: {plan.grid_rows}x{plan.grid_cols} grid, "
                        f"{len(plan.waypoints)} waypoints, est. {plan.estimated_duration_min}min"
                    )
                    # Write MAVLink mission JSON alongside other control-plane files
                    output_base = Path(__file__).resolve().parent.parent.parent / "output" / "caesar" / "control_plane"
                    try:
                        output_base.mkdir(parents=True, exist_ok=True)
                        mission = planner.plan_to_mavlink_json(plan, min(lats), min(lons))
                        (output_base / "ast_mission.json").write_text(
                            json.dumps(mission, indent=2), encoding="utf-8"
                        )
                    except Exception as _ast_write_err:
                        print(f"[orchestrator] WARNING: could not write ast_mission.json: {_ast_write_err}")
            # ──────────────────────────────────────────────────────────────────
            relay_actions.append(relay_action)

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
        "multi_domain_orchestration": {
            "framework": "MARL (Multi-Agent Reinforcement Learning)",
            "integrated_assets": ["BAHA (Air)", "BARKAN (Land)", "SANCAR (Sea)", "Archer (VTOL)", "Duma (UGV)"],
            "decentralized_fallback": True,
        }
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
                    "job_type": "distributed_gaussian_process_refresh",
                    "target_model": "pxADMM-environmental-anomaly-detector",
                    "window_size": max(50, len(tracks) * 5),
                    "trigger": "confidence-spread-shift",
                }
            )
        if "rl" in node["learning_layers"]:
            reinforcement_jobs.append(
                {
                    "node_id": node["node_id"],
                    "job_type": "swarm_routing_policy_update",
                    "target_policy": "aco_pso_traffic_coordinator",
                    "resource_allocation_algorithm": "ABC", # Artificial Bee Colony for resource distribution
                    "reward_signal": "alert_delivery_latency_vs_bandwidth",
                    "trigger": "relay-load-change",
                }
            )
            
        if "compression" in node["learning_layers"]:
            supervised_jobs.append(
                {
                    "node_id": node["node_id"],
                    "job_type": "model_compression",
                    "technique": "LAP-DTR",  # Layer-Adaptive Partitioning with Dynamic Task Redistribution
                    "knowledge_distillation": True,
                    "trigger": "bandwidth-saturation-limit",
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
            "strategy": "FedPDM_Sinkhorn_Knopp",
            "participants": participants,
            "aggregation_target": "regional-hub",
            "global_models": [
                "yolo-world-detector-head",
                "pxADMM-environmental-anomaly-detector",
                "aco_pso_traffic_coordinator",
            ],
        },
    }


def highest_pressure_zone(records: list[dict]) -> str:
    if not records:
        return "idle"
    zone_counts = Counter(
        record["envelope"]["body"]["site"]
        for record in records
        if "envelope" in record and "body" in record["envelope"] and "site" in record["envelope"]["body"]
    )
    return zone_counts.most_common(1)[0][0] if zone_counts else "idle"


def write_json(path: Path, payload: dict) -> None:
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def append_jsonl(path: Path, payload: dict) -> None:
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(payload) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
