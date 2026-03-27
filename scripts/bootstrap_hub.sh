#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${1:-/opt/uriel-caesar}"

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
  git \
  curl

if ! command -v rustup >/dev/null 2>&1; then
  echo "[bootstrap-hub] Installing Rust via rustup"
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi

source "$HOME/.cargo/env"

echo "[bootstrap-hub] Creating console Python environment"
python3 -m venv "$REPO_DIR/.venv-console"
source "$REPO_DIR/.venv-console/bin/activate"
python -m pip install --upgrade pip

mkdir -p "$REPO_DIR/output/caesar"

echo "[bootstrap-hub] Hub prerequisites installed"
echo "[bootstrap-hub] Next actions:"
echo "  1. Set trusted_public_keys in configs/hub-dev.toml"
echo "  2. Run: cargo run -p caesar-hub -- --config configs/hub-dev.toml serve"
echo "  3. Run: python services/caesar_console/server.py --host 0.0.0.0 --port 8090"
