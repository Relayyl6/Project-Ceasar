#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${1:-/opt/uriel-caesar}"

echo "[bootstrap-edge] Updating apt cache"
sudo apt-get update

echo "[bootstrap-edge] Installing system packages"
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  python3 \
  python3-pip \
  python3-venv \
  git \
  curl \
  ffmpeg

if ! command -v rustup >/dev/null 2>&1; then
  echo "[bootstrap-edge] Installing Rust via rustup"
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi

source "$HOME/.cargo/env"

echo "[bootstrap-edge] Creating Python environment"
python3 -m venv "$REPO_DIR/.venv-edge"
source "$REPO_DIR/.venv-edge/bin/activate"
python -m pip install --upgrade pip
python -m pip install -r "$REPO_DIR/requirements-edge.txt"
python -m pip install pynacl ultralytics onnx

echo "[bootstrap-edge] Edge prerequisites installed"
echo "[bootstrap-edge] Next manual actions:"
echo "  1. Connect the Raspberry Pi camera ribbon or USB/V4L2 camera"
echo "  2. Copy/export your ONNX model into $REPO_DIR/models/"
echo "  3. Edit configs/edge-pi.toml or configs/edge-v4l2.toml"
echo "  4. Run: cargo run -p uriel-edge-node -- --config configs/edge-pi.toml"
