#!/usr/bin/env bash
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODEL="$ROOT/models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01"
python3 "$ROOT/tools/make_vocab.py"
printf 'HEY ASUNA\n' > "$ROOT/keywords/plain_asuna.txt"
printf 'HEY AS SOON\n' > "$ROOT/keywords/plain_heassoon.txt"
python3 "$ROOT/tools/text2token.py" "$MODEL/bpe.model" \
  "$ROOT/keywords/plain_heassoon.txt" "$ROOT/keywords/tok_heassoon.txt"
echo "--- tokenized dosya icerigi:"
grep . "$ROOT/keywords/tok_heassoon.txt"
cd "$ROOT/kws"
cargo build --release --bin kws-tokentest 2>&1 | tail -3
echo "--- test:"
./target/release/kws-tokentest "$MODEL" \
  "$ROOT/keywords/tok_heassoon.txt" \
  "$ROOT/keywords/plain_heassoon.txt" \
  "$ROOT/audio/pos_en/002_Daniel_r-.wav"
