import argparse
import re
import subprocess
import sys
from pathlib import Path

# Allowlist: only safe model variant names (alphanumeric, hyphens, underscores).
_VARIANT_RE = re.compile(r'^[a-zA-Z0-9][a-zA-Z0-9_\-]{0,63}$')


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Fetch or export a free YOLO ONNX model")
    parser.add_argument("--variant", default="yolov8n", help="Base model variant, for example yolov8n")
    parser.add_argument("--repo-root", default=".", help="Repository root")
    parser.add_argument("--imgsz", type=int, default=640, help="Export image size")
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    # Validate variant before using it inside a Python code string (injection guard).
    if not _VARIANT_RE.match(args.variant):
        raise SystemExit(
            f"Invalid --variant '{args.variant}'. Only alphanumeric characters, hyphens, and underscores are allowed."
        )

    repo_root = Path(args.repo_root).resolve()
    model_dir = repo_root / "models"
    try:
        model_dir.mkdir(parents=True, exist_ok=True)
    except Exception as e:
        raise SystemExit(f"Failed to create model directory {model_dir}: {e}")
    output_path = model_dir / f"{args.variant}.onnx"

    script = f"""
from ultralytics import YOLO
model = YOLO('{args.variant}.pt')
model.export(format='onnx', imgsz={args.imgsz}, opset=12, simplify=True)
"""

    # Timeout: 30 minutes is generous for model download + export.
    try:
        subprocess.run([sys.executable, "-c", script], check=True, cwd=repo_root, timeout=1800)
    except subprocess.CalledProcessError as e:
        raise SystemExit(f"Model export failed (possibly due to network issue downloading the model): {e}")
    except subprocess.TimeoutExpired:
        raise SystemExit("Model export timed out")

    exported = repo_root / f"{args.variant}.onnx"
    if exported.exists():
        try:
            exported.replace(output_path)
        except Exception as e:
            raise SystemExit(f"Failed to move exported model to {output_path}: {e}")
        print(output_path)
        return 0

    raise SystemExit(f"Expected export at {exported}, but it was not created")


if __name__ == "__main__":
    raise SystemExit(main())
