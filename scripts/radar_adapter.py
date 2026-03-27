import argparse
import json
import time
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Radar adapter contract for the Uriel edge node")
    parser.add_argument("--mode", choices=["synthetic", "json"], default="synthetic")
    parser.add_argument("--points", type=int, default=48)
    parser.add_argument("--input", help="JSON input file for non-synthetic modes")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.mode == "synthetic":
        points = [
            {
                "range_m": 18.0 + (index % 30),
                "azimuth_deg": -18.0 + index * 0.75,
                "radial_velocity_mps": 2.5 + (index % 5),
            }
            for index in range(args.points)
        ]
    else:
        if not args.input:
            raise SystemExit("--input is required when mode=json")
        points = json.loads(Path(args.input).read_text(encoding="utf-8"))["points"]

    json.dump(
        {
            "timestamp_ms": int(time.time() * 1000),
            "points": points,
        },
        fp=sys.stdout,
    )
    return 0


if __name__ == "__main__":
    import sys

    raise SystemExit(main())
