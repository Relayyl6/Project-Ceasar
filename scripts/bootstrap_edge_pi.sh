#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${1:-/opt/uriel-caesar}"

# Fail fast if the repository directory does not exist.
if [[ ! -d "$REPO_DIR" ]]; then
  echo "[bootstrap-edge] ERROR: REPO_DIR '$REPO_DIR' does not exist. Clone the repo first." >&2
  exit 1
fi

echo "[bootstrap-edge] Updating apt cache"
sudo apt-get update

echo "[bootstrap-edge] Installing system packages"
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  libclang-dev \
  libopencv-dev \
  libpcap-dev \
  mosquitto \
  mosquitto-clients \
  python3 \
  python3-pip \
  python3-venv \
  git \
  curl \
  ffmpeg \
  python3-smbus \
  i2c-tools \
  rpicam-apps

echo "[bootstrap-edge] Enabling and starting mosquitto MQTT broker"
sudo systemctl enable --now mosquitto

if ! command -v rustup > /dev/null 2>&1; then
  echo "[bootstrap-edge] Installing Rust via rustup"
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi

source "$HOME/.cargo/env"

echo "[bootstrap-edge] Creating Python environment for sensor adapters"
python3 -m venv "$REPO_DIR/.venv-edge"
source "$REPO_DIR/.venv-edge/bin/activate"
python -m pip install --upgrade pip
python -m pip install -r "$REPO_DIR/requirements-edge.txt"

echo ""
echo "[bootstrap-edge] === Pi 3 Edge Node Setup Complete ==="
echo ""
echo "  IMPORTANT: Edit configs/edge-pi3.toml before launching."
echo "  Set the hub PC's IP address in two places:"
echo "    [uplink]   tcp_addr    = \"<HUB_IP>:7878\""
echo "    [inference] ollama_endpoint = \"http://<HUB_IP>:9090\""
echo ""
echo "  The hub PC must be running:"
echo "    - cargo run -p caesar-hub -- --config configs/hub-dev.toml serve"
echo "    - python services/remote_infer_server.py --port 9090"
echo "    - python services/caesar_console/server.py --port 8090"
echo ""
echo "  Then launch the edge node:"
echo "    cargo build --release -p uriel-edge-node"
echo "    ./target/release/uriel-edge-node --config configs/edge-pi3.toml"
echo ""
echo "  NOTE: No ONNX models are needed on the Pi 3."
echo "        Inference is offloaded to the hub PC via remote_http mode."
echo "        If the hub is unreachable, the Pi 3 falls back to local heuristic automatically."
