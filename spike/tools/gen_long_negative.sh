#!/usr/bin/env bash
# ASU-008b — uzun surekli negatif konusma akisi (false-accept oranini
# 42 saniyelik korpustan cok daha anlamli bir tabana tasimak icin).
# Ayni metin 4 farkli sesle okunur.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/audio_long"
rm -rf "$OUT"
mkdir -p "$OUT"
TXT="$ROOT/tools/long_negative.txt"

i=0
for v in "Samantha" "Daniel" "Karen" "Yelda"; do
  i=$((i + 1))
  f=$(printf '%s/long_%d_%s.wav' "$OUT" "$i" "$(printf '%s' "$v" | tr -cd '[:alnum:]')")
  say -v "$v" -f "$TXT" --file-format=WAVE --data-format=LEI16@16000 -o "$f"
  echo "$f"
done
