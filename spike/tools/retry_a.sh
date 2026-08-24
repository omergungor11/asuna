#!/usr/bin/env bash
# ASU-008b — CASE A tekrari: temiz target dizini + ag ile indirme guvenilir mi?
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="$ROOT/buildtest/A2"
rm -rf "$DIR"
export CARGO_TARGET_DIR="$DIR"
cd "$ROOT/kws"
start=$SECONDS
cargo build --release --bin kws-batch > "$ROOT/buildtest/A2.log" 2>&1
rc=$?
echo "sure: $((SECONDS - start))s exit=$rc"
grep -c "Downloading sherpa-onnx libs" "$ROOT/buildtest/A2.log" || true
tail -6 "$ROOT/buildtest/A2.log"
ls -la "$DIR/sherpa-onnx-prebuilt" 2>/dev/null || true
