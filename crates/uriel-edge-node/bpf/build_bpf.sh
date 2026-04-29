#!/usr/bin/env bash
# build_bpf.sh — Compile eBPF programs for uriel-edge-node recon module.
# Run this on the Linux edge node before starting uriel-edge-node.
# Requires: clang, llvm, linux-headers-$(uname -r), bpftool
#
# Usage:  bash crates/uriel-edge-node/bpf/build_bpf.sh

set -euo pipefail

BPF_SRC="crates/uriel-edge-node/bpf"
BPF_OUT="target/bpf"

if ! command -v clang &>/dev/null; then
    echo "[bpf] ERROR: clang not found. Install with: sudo apt install clang llvm"
    exit 1
fi

mkdir -p "$BPF_OUT"

echo "[bpf] Compiling sys_enter.bpf.c ..."
clang -O2 -target bpf \
    -I/usr/include/$(uname -m)-linux-gnu \
    -c "$BPF_SRC/sys_enter.bpf.c" \
    -o "$BPF_OUT/sys_enter.bpf.o"
echo "[bpf] -> $BPF_OUT/sys_enter.bpf.o OK"

echo "[bpf] Compiling ssl_read.bpf.c ..."
clang -O2 -target bpf \
    -I/usr/include/$(uname -m)-linux-gnu \
    -c "$BPF_SRC/ssl_read.bpf.c" \
    -o "$BPF_OUT/ssl_read.bpf.o"
echo "[bpf] -> $BPF_OUT/ssl_read.bpf.o OK"

echo "[bpf] All eBPF programs compiled. Place edge node binary and run as root (required for BPF)."
