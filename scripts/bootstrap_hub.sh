#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${1:-/opt/uriel-caesar}"

# Fail fast if the repository directory does not exist.
if [[ ! -d "$REPO_DIR" ]]; then
  echo "[bootstrap-hub] ERROR: REPO_DIR '$REPO_DIR' does not exist. Clone the repo first." >&2
  exit 1
fi

echo "[bootstrap-hub] Updating apt cache"
sudo apt-get update

echo "[bootstrap-hub] Installing hub packages"
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  python3 \
  python3-pip \
  python3-venv \
  mosquitto \
  mosquitto-clients \
  git \
  curl

echo "[bootstrap-hub] Enabling and starting mosquitto MQTT broker"
sudo systemctl enable --now mosquitto

if ! command -v rustup > /dev/null 2>&1; then
  echo "[bootstrap-hub] Installing Rust via rustup"
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi

source "$HOME/.cargo/env"

echo "[bootstrap-hub] Installing Ollama (VLM for vision + thermal/radar inference)"
if ! command -v ollama > /dev/null 2>&1; then
  curl -fsSL https://ollama.com/install.sh | sh
fi

echo "[bootstrap-hub] Pulling Ollama vision model (llava) — needed for optical inference"
ollama pull llava

echo "[bootstrap-hub] Creating console Python environment"
python3 -m venv "$REPO_DIR/.venv-console"
source "$REPO_DIR/.venv-console/bin/activate"
python -m pip install --upgrade pip
python -m pip install paho-mqtt opencv-python pillow requests

mkdir -p "$REPO_DIR/output/caesar"

echo "[bootstrap-hub] Hub prerequisites installed"
echo ""
echo "[bootstrap-hub] === LAUNCH ORDER ==="
echo ""
echo "  1. Start the Rust hub (receives signed envelopes from edge nodes):"
echo "     cargo run -p caesar-hub -- --config configs/hub-dev.toml serve"
echo ""
echo "  2. Start the Remote Inference Server (for Pi 3 nodes):"
echo "     python services/remote_infer_server.py --port 9090"
echo "     (This accepts JPEG frames from Pi 3 nodes and runs Ollama VLM locally)"
echo ""
echo "  3. Start the Caesar Console dashboard:"
echo "     python services/caesar_console/server.py --host 0.0.0.0 --port 8090 --mqtt-broker <HUB_IP>"
echo "     Replace <HUB_IP> with this machine's LAN IP (same one in edge configs)."
echo "     Then open http://localhost:8090 in your browser"
echo ""
echo "  4. On Raspberry Pi 3 — run the edge node:"
echo "     cargo build --release"
echo "     ./target/release/uriel-edge-node --config configs/edge-pi3.toml"
echo ""
echo "  Hub PC IP on local network: run 'ip address' or 'ipconfig' to find it."
echo "  Update tcp_addr and ollama_endpoint in configs/edge-pi3.toml with that IP."
