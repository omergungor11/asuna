#!/usr/bin/env bash
# ASU-008b — idle CPU% / RSS olcumu.
# Kullanim: measure.sh <etiket> <saniye> <mic|loop> [wav]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LABEL="$1"; SECS="$2"; MODE="$3"; WAV="${4:-}"
MODEL="$ROOT/models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01"
KW="$ROOT/keywords/keywords_asuna.txt"
OUT="$ROOT/measurements"
mkdir -p "$OUT"

"$ROOT/kws/target/release/kws-idle" "$MODE" "$MODEL" "$KW" "$SECS" $WAV \
  > "$OUT/$LABEL.log" 2>&1 &
PID=$!
sleep 3
printf 't_s\tcpu_pct\trss_kb\tthreads\n' > "$OUT/$LABEL.samples.tsv"
t=0
while kill -0 "$PID" 2>/dev/null; do
  line=$(ps -o %cpu=,rss=,nlwp= -p "$PID" 2>/dev/null | tr -s ' ')
  if [ -z "$line" ]; then break; fi
  printf '%s\t%s\n' "$t" "$(printf '%s' "$line" | awk '{print $1"\t"$2"\t"$3}')" \
    >> "$OUT/$LABEL.samples.tsv"
  sleep 5
  t=$((t + 5))
done
wait "$PID"
echo "olcum bitti: $OUT/$LABEL.samples.tsv"
