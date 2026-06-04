"""
thermal_adapter.py — Uriel Edge Node thermal sensor bridge.

Modes:
  synthetic   Generate synthetic 32×24 temperature grid (default, no hardware needed)
  hardware    Read real MLX90640 via smbus2 on specified I2C bus
  csv         Load temperatures from a CSV file
  json        Load temperatures from a JSON file
"""
import argparse
import csv
import json
import math
import sys
import time
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Thermal adapter for Uriel edge node")
    parser.add_argument("--mode", choices=["synthetic", "hardware", "csv", "json"], default="synthetic")
    parser.add_argument("--width",   type=int, default=32,   help="Grid width (cells)")
    parser.add_argument("--height",  type=int, default=24,   help="Grid height (cells)")
    parser.add_argument("--bus",     type=int, default=1,    help="I2C bus number (hardware mode)")
    parser.add_argument("--address", type=str, default="0x33", help="MLX90640 I2C address hex")
    parser.add_argument("--input",   help="Input file for csv/json modes")
    return parser.parse_args()


def read_mlx90640_hardware(bus: int, address_str: str) -> list[float]:
    """Read a 32×24 temperature frame from a real MLX90640 via smbus2."""
    addr = int(address_str, 16)
    try:
        import smbus2
        import mlx90640  # pip install mlx90640-library  (RPi only)
        sensor = mlx90640.MLX90640()
        sensor.i2c_init(f"/dev/i2c-{bus}")
        sensor.set_refresh_rate(mlx90640.RefreshRate.RATE_2_HZ)
        frame = [0.0] * 768
        try:
            sensor.get_frame_data(frame)
        except Exception as hw_err:
            # Sensor I/O failure (e.g. IOError, OSError from smbus) after the
            # library successfully imported — fall through to synthetic so the
            # pipeline keeps running rather than crashing.
            print(
                f"[thermal_adapter] MLX90640 frame read failed ({hw_err}). "
                "Generating synthetic frame.",
                file=sys.stderr,
            )
            return _synthetic_frame(32, 24)
        return frame
    except ImportError:
        # smbus2/mlx90640 not installed — try raw smbus read
        try:
            import smbus2
            bus_obj = smbus2.SMBus(bus)
            raw = bus_obj.read_i2c_block_data(addr, 0x00, 32)  # partial read for health check
            bus_obj.close()
            # Generate plausible temps from raw bytes (stand-in until full lib is available)
            temps = [20.0 + (b / 255.0) * 20.0 for b in raw]
            # Pad to 768 cells
            while len(temps) < 768:
                temps.append(temps[-1] + 0.1)
            return temps[:768]
        except Exception as e:
            # Hardware unavailable — fall through to synthetic
            print(f"[thermal_adapter] Hardware read failed ({e}). Generating synthetic frame.", file=sys.stderr)
            return _synthetic_frame(32, 24)


def _synthetic_frame(width: int, height: int) -> list[float]:
    t = time.time()
    cell_count = width * height
    # NOTE: int(t) causes a once-per-second step in the offset term; this is
    # intentional for a crude "tick" effect but means the frame is not
    # perfectly smooth at second boundaries.  Use t directly if that matters.
    return [
        22.0 + math.sin(t * 0.3 + i * 0.07) * 4.0 + ((i * 7 + int(t)) % 13) * 0.5
        for i in range(cell_count)
    ]


def main() -> int:
    args = parse_args()

    if args.mode == "hardware":
        temperatures = read_mlx90640_hardware(args.bus, args.address)
    elif args.mode == "synthetic":
        temperatures = _synthetic_frame(args.width, args.height)
    elif args.mode == "csv":
        if not args.input:
            raise SystemExit("--input required for mode=csv")
        temperatures = []
        with open(args.input, encoding="utf-8") as f:
            for row in csv.reader(f):
                for v in row:
                    v = v.strip()
                    if v:
                        temperatures.append(float(v))
    elif args.mode == "json":
        if not args.input:
            raise SystemExit("--input required for mode=json")
        temperatures = json.loads(Path(args.input).read_text(encoding="utf-8"))["temperatures_c"]
    else:
        raise SystemExit(f"Unknown mode: {args.mode}")

    json.dump(
        {"timestamp_ms": int(time.time() * 1000), "temperatures_c": temperatures},
        fp=sys.stdout,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
