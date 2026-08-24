#!/usr/bin/env bash
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODEL="$ROOT/models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01"
cd "$ROOT/kws"
cargo build --release --bin kws-tokentest 2>&1 | tail -2
export SKIP_CASE2=1
./target/release/kws-tokentest "$MODEL" \
  "$ROOT/keywords/tok_heassoon.txt" \
  "$ROOT/keywords/plain_heassoon.txt" \
  "$ROOT/audio/pos_en/002_Daniel_r-.wav"
echo "exit=$?"
