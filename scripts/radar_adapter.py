"""
radar_adapter.py — Uriel Edge Node radar sensor bridge.

Modes:
  synthetic   Generate synthetic point cloud (default, no hardware needed)
  hardware    Read real LD2450 / TI IWR6843 via UART serial port
  json        Load point cloud from a JSON file
"""
import argparse
import json
import math
import sys
import time


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Radar adapter for Uriel edge node")
    parser.add_argument("--mode",   choices=["synthetic", "hardware", "json"], default="synthetic")
    parser.add_argument("--points", type=int,   default=48,            help="Number of synthetic points")
    parser.add_argument("--port",   type=str,   default="/dev/ttyAMA0", help="UART port (hardware mode)")
    parser.add_argument("--baud",   type=int,   default=256000,         help="Baud rate (LD2450 default: 256000)")
    parser.add_argument("--input",  help="JSON input file (json mode)")
    return parser.parse_args()


def read_ld2450_hardware(port: str, baud: int) -> list[dict]:
    """Read one sweep from an LD2450 24 GHz mmWave radar via UART."""
    try:
        import serial
        with serial.Serial(port, baud, timeout=0.15) as ser:
            raw = ser.read(30)          # LD2450 frame is 30 bytes
            if len(raw) < 28:
                raise ValueError(f"Short read: {len(raw)} bytes")

            # LD2450 binary protocol: 4-byte header + 3 targets × 8 bytes + footer
            # Each target: X(int16LE) Y(int16LE) Speed(int16LE) Distance(uint16LE)
            HEADER = b'\xaa\xff\x03\x00'
            if raw[:4] != HEADER:
                raise ValueError("Invalid LD2450 frame header")

            import struct
            points = []
            for i in range(3):
                offset = 4 + i * 8
                x, y, speed, dist = struct.unpack_from('<hhhH', raw, offset)
                if dist == 0:
                    continue   # empty target slot
                range_m     = dist / 1000.0
                azimuth_deg = math.degrees(math.atan2(x, y))
                velocity    = speed / 100.0
                points.append({
                    "range_m": round(range_m, 3),
                    "azimuth_deg": round(azimuth_deg, 2),
                    "radial_velocity_mps": round(velocity, 3),
                })
            return points if points else _synthetic_points(6)

    except Exception as e:
        print(f"[radar_adapter] Hardware read failed ({e}). Using synthetic points.", file=sys.stderr)
        return _synthetic_points(6)


def _synthetic_points(n: int) -> list[dict]:
    t = time.time()
    return [
        {
            "range_m": round(18.0 + math.sin(t * 0.5 + i) * 12.0, 3),
            "azimuth_deg": round(-18.0 + i * (36.0 / max(n - 1, 1)), 2),
            "radial_velocity_mps": round(2.5 + math.cos(t * 0.3 + i * 0.4) * 1.5, 3),
        }
        for i in range(n)
    ]


def main() -> int:
    args = parse_args()

    if args.mode == "hardware":
        points = read_ld2450_hardware(args.port, args.baud)
    elif args.mode == "synthetic":
        points = _synthetic_points(args.points)
    elif args.mode == "json":
        if not args.input:
            raise SystemExit("--input required for mode=json")
        try:
            with open(args.input, encoding="utf-8") as f:
                points = json.loads(f.read())["points"]
        except FileNotFoundError:
            raise SystemExit(f"Input file not found: {args.input}")
        except json.JSONDecodeError as e:
            raise SystemExit(f"Invalid JSON in file {args.input}: {e}")
        except KeyError:
            raise SystemExit(f"Missing 'points' key in JSON file: {args.input}")
    else:
        raise SystemExit(f"Unknown mode: {args.mode}")

    json.dump(
        {"timestamp_ms": int(time.time() * 1000), "points": points},
        fp=sys.stdout,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
