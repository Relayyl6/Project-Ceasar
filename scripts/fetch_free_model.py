import argparse
import subprocess
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Fetch or export a free YOLO ONNX model")
    parser.add_argument("--variant", default="yolov8n", help="Base model variant, for example yolov8n")
    parser.add_argument("--repo-root", default=".", help="Repository root")
    parser.add_argument("--imgsz", type=int, default=640, help="Export image size")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path(args.repo_root).resolve()
    model_dir = repo_root / "models"
    model_dir.mkdir(parents=True, exist_ok=True)
    output_path = model_dir / f"{args.variant}.onnx"

    script = f"""
from ultralytics import YOLO
model = YOLO('{args.variant}.pt')
model.export(format='onnx', imgsz={args.imgsz}, opset=12, simplify=True)
"""

    subprocess.run([sys.executable, "-c", script], check=True, cwd=repo_root)

    exported = repo_root / f"{args.variant}.onnx"
    if exported.exists():
        exported.replace(output_path)
        print(output_path)
        return 0

    raise SystemExit(f"Expected export at {exported}, but it was not created")


if __name__ == "__main__":
    raise SystemExit(main())
