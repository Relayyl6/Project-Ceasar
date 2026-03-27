import argparse
import csv
import json
import time
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Thermal adapter contract for the Uriel edge node")
    parser.add_argument("--mode", choices=["synthetic", "csv", "json"], default="synthetic")
    parser.add_argument("--width", type=int, default=640)
    parser.add_argument("--height", type=int, default=512)
    parser.add_argument("--input", help="CSV or JSON input file for non-synthetic modes")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.mode == "synthetic":
        cell_count = max(1, (args.width // 16) * (args.height // 16))
        temperatures = [24.0 + ((index % 19) * 0.65) for index in range(cell_count)]
    elif args.mode == "csv":
        if not args.input:
            raise SystemExit("--input is required when mode=csv")
        temperatures = read_csv(Path(args.input))
    else:
        if not args.input:
            raise SystemExit("--input is required when mode=json")
        temperatures = json.loads(Path(args.input).read_text(encoding="utf-8"))["temperatures_c"]

    json.dump(
        {
            "timestamp_ms": int(time.time() * 1000),
            "temperatures_c": temperatures,
        },
        fp=sys.stdout,
    )
    return 0


def read_csv(path: Path) -> list[float]:
    values: list[float] = []
    with path.open("r", encoding="utf-8") as handle:
        reader = csv.reader(handle)
        for row in reader:
            for value in row:
                stripped = value.strip()
                if stripped:
                    values.append(float(stripped))
    return values


if __name__ == "__main__":
    import sys

    raise SystemExit(main())
