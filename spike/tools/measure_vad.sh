#!/usr/bin/env bash
# ASU-008b — VAD-kapili idle olcumu (measure.sh'in micvad varyanti).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LABEL="$1"; SECS="$2"
MODEL="$ROOT/models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01"
KW="$ROOT/keywords/keywords_asuna.txt"
VAD="$ROOT/models/silero_vad.onnx"
OUT="$ROOT/measurements"
mkdir -p "$OUT"

"$ROOT/kws/target/release/kws-idle" micvad "$MODEL" "$KW" "$SECS" "$VAD" \
  > "$OUT/$LABEL.log" 2>&1 &
PID=$!
sleep 3
printf 't_s\tcpu_pct\trss_kb\n' > "$OUT/$LABEL.samples.tsv"
t=0
while kill -0 "$PID" 2>/dev/null; do
  line=$(ps -o %cpu=,rss= -p "$PID" 2>/dev/null | tr -s ' ')
  [ -z "$line" ] && break
  printf '%s\t%s\n' "$t" "$(printf '%s' "$line" | awk '{print $1"\t"$2}')" \
    >> "$OUT/$LABEL.samples.tsv"
  sleep 5
  t=$((t + 5))
done
wait "$PID"
echo "olcum bitti: $OUT/$LABEL.samples.tsv"
