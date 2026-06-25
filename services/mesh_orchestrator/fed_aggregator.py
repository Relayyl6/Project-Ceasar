"""
Federated Averaging (FedAvg) aggregator for Project Caesar.

Nodes contribute per-class confidence statistics; the hub computes
aggregated threshold recommendations and distributes them back.

This is genuine federated learning of inference parameters: the
"model" being trained is the per-class confidence threshold used by
each edge node's inference engine.  No raw video or sensor data is
ever shared — only the lightweight statistics derived from local
inference runs.

Contribution format (what each node POSTs / writes):
{
    "node_id":      "uriel-tower-01",
    "timestamp":    1717000000.0,      # Unix epoch float
    "sample_count": 1234,              # total observations used
    "class_stats": {
        "person": {
            "mean_confidence": 0.82,
            "n_detections": 412
        },
        "vehicle": {
            "mean_confidence": 0.74,
            "n_detections": 90
        }
    }
}

Global model format (written by aggregate(), read by nodes):
{
    "version":         1717000000,
    "aggregated_at":   "2024-05-29T12:00:00Z",
    "n_contributors":  3,
    "global_thresholds": {
        "person": {
            "recommended_threshold":  0.656,   # 80% of global mean
            "global_mean_confidence": 0.82,
            "total_detections":       1236,
            "contributing_nodes":     3
        }
    }
}
"""

import json
import time
from pathlib import Path
from typing import Dict, List, Optional


# Maximum age of a contribution before it is considered stale and ignored.
_CONTRIBUTION_MAX_AGE_S = 86400  # 24 hours

# FedAvg conservatism factor: threshold = mean_confidence * factor.
# Set to 0.80 so we only flag detections above 80 % of the network's
# average confidence — a deliberately conservative posture.
_FEDAVG_CONSERVATISM = 0.80

# Hard limits on the recommended threshold.
_THRESHOLD_MIN = 0.30
_THRESHOLD_MAX = 0.95


class FedAggregator:
    """
    Federated Averaging aggregator for per-class detection thresholds.

    Thread-safety: each public method is stateless with respect to the
    instance (state lives on disk).  Callers that invoke ``aggregate``
    and ``contribute`` concurrently from different threads should use
    an external lock or rely on atomic file-write semantics.
    """

    def __init__(self, contributions_dir: Path, global_model_path: Path) -> None:
        self.contributions_dir = contributions_dir
        self.global_model_path = global_model_path
        # Iteration 2 check: create the dir so glob() doesn't raise even on first run.
        contributions_dir.mkdir(parents=True, exist_ok=True)

    # ------------------------------------------------------------------ #
    #  Public API                                                          #
    # ------------------------------------------------------------------ #

    def aggregate(self) -> Dict:
        """
        Read all node contributions and compute FedAvg of confidence thresholds.

        Returns the global model dict (also persisted to disk).
        Returns an empty dict if no valid contributions exist — this is
        always safe (callers should treat an empty result as "no update").

        Iteration 2 guarantee: a missing or empty contributions directory
        does NOT raise; it simply returns {}.
        """
        contributions = self._load_contributions()

        if not contributions:
            # No valid data yet — return empty dict, do not overwrite an
            # existing global model with a blank one.
            return {}

        # ── FedAvg: weighted average by sample_count ────────────────────
        # For each class, accumulate:
        #   class_sums[cls]    = sum(mean_confidence_i * weight_i)
        #   class_weights[cls] = sum(weight_i)
        #   class_counts[cls]  = sum(n_detections_i)
        class_sums: Dict[str, float] = {}
        class_weights: Dict[str, float] = {}
        class_counts: Dict[str, int] = {}

        for contrib in contributions:
            # Weight each node's contribution by its total observation count.
            weight = float(contrib.get("sample_count", 1) or 1)
            for cls, stats in contrib.get("class_stats", {}).items():
                mean_conf = float(stats.get("mean_confidence", 0.5))
                n = int(stats.get("n_detections", 0))
                if n == 0:
                    continue  # skip classes with no actual detections
                if cls not in class_sums:
                    class_sums[cls] = 0.0
                    class_weights[cls] = 0.0
                    class_counts[cls] = 0
                class_sums[cls] += mean_conf * weight
                class_weights[cls] += weight
                class_counts[cls] += n

        # ── Derive thresholds ───────────────────────────────────────────
        global_thresholds: Dict[str, dict] = {}
        for cls in class_sums:
            if class_weights[cls] <= 0:
                continue
            avg = class_sums[cls] / class_weights[cls]
            raw_threshold = avg * _FEDAVG_CONSERVATISM
            recommended = round(
                max(_THRESHOLD_MIN, min(_THRESHOLD_MAX, raw_threshold)), 3
            )
            global_thresholds[cls] = {
                "recommended_threshold": recommended,
                "global_mean_confidence": round(avg, 4),
                "total_detections": class_counts[cls],
                "contributing_nodes": len(contributions),
            }

        result: Dict = {
            "version": int(time.time()),
            "aggregated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "n_contributors": len(contributions),
            "global_thresholds": global_thresholds,
        }

        # Atomic-ish write: write to a .tmp file then rename so readers
        # never see a partially-written global model.
        tmp = self.global_model_path.with_suffix(".tmp")
        tmp.write_text(json.dumps(result, indent=2), encoding="utf-8")
        tmp.replace(self.global_model_path)

        return result

    def contribute(self, node_id: str, stats: Dict) -> None:
        """
        Persist a node's contribution to disk.

        ``stats`` should contain at minimum:
            "sample_count": int
            "class_stats":  {class_label: {"mean_confidence": float, "n_detections": int}}

        node_id and timestamp are injected automatically.
        """
        if not node_id or not isinstance(node_id, str):
            raise ValueError("node_id must be a non-empty string")

        payload = dict(stats)
        payload["node_id"] = node_id
        payload["timestamp"] = time.time()

        out = self.contributions_dir / f"{node_id}.json"
        # Write to tmp first, then rename — prevents corrupt reads during write.
        tmp = out.with_suffix(".tmp")
        tmp.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        tmp.replace(out)

    def load_global_model(self) -> Dict:
        """
        Load and return the current global model from disk.
        Returns a minimal sentinel dict if the file does not exist yet.
        """
        if not self.global_model_path.exists():
            return {"version": 0, "global_thresholds": {}}
        try:
            return json.loads(
                self.global_model_path.read_text(encoding="utf-8")
            )
        except Exception:
            return {"version": 0, "global_thresholds": {}}

    # ------------------------------------------------------------------ #
    #  Private helpers                                                     #
    # ------------------------------------------------------------------ #

    def _load_contributions(self) -> List[Dict]:
        """
        Glob all *.json files in the contributions directory, parse them,
        and filter out stale entries (older than _CONTRIBUTION_MAX_AGE_S).

        Corrupt or unreadable files are silently skipped so a single bad
        contributor cannot break the aggregation cycle.
        """
        now = time.time()
        contributions: List[Dict] = []

        # Iteration 2: contributions_dir is guaranteed to exist (created in
        # __init__), so glob never raises even if the directory was just made.
        for f in self.contributions_dir.glob("*.json"):
            try:
                data = json.loads(f.read_text(encoding="utf-8"))
                age = now - float(data.get("timestamp", 0))
                if age > _CONTRIBUTION_MAX_AGE_S:
                    # Stale contribution — skip but don't delete (in case
                    # the system clock is temporarily wrong on the hub).
                    continue
                contributions.append(data)
            except Exception:
                # Corrupt JSON or I/O error — skip silently.
                pass

        return contributions
