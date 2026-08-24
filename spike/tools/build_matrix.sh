#!/usr/bin/env bash
# ASU-008b — build script'inin CI / offline davranisi.
#  A) temiz target + ag  -> arsivi indirir mi, ne kadar surer
#  B) temiz target + SHERPA_ONNX_ARCHIVE_DIR (vendored arsiv) -> indirme YOK
#  C) temiz target + ag YOK (bozuk proxy) + vendor YOK -> hata mesaji nasil
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CRATE="$ROOT/kws"
WORK="$ROOT/buildtest"
VENDOR="$ROOT/vendor-archive"
ARCHIVE="sherpa-onnx-v1.13.5-osx-arm64-static-lib.tar.bz2"

mkdir -p "$VENDOR"
if [ ! -f "$VENDOR/$ARCHIVE" ]; then
  cp "$CRATE/target/sherpa-onnx-prebuilt/$ARCHIVE" "$VENDOR/$ARCHIVE" 2>/dev/null \
    || echo "UYARI: vendor arsivi kopyalanamadi"
fi

run_case() {
  local name="$1"; shift
  local dir="$WORK/$name"
  rm -rf "$dir"; mkdir -p "$dir"
  echo "=================== CASE $name"
  local start=$SECONDS
  ( cd "$CRATE" && env CARGO_TARGET_DIR="$dir" "$@" cargo build --release --bin kws-batch ) \
    > "$WORK/$name.log" 2>&1
  local rc=$?
  echo "sure: $((SECONDS - start))s  exit=$rc"
  if grep -q "Downloading sherpa-onnx libs" "$WORK/$name.log"; then
    echo "indirme: EVET"
  else
    echo "indirme: HAYIR"
  fi
  if [ $rc -ne 0 ]; then
    echo "--- hata (son 12 satir):"
    tail -12 "$WORK/$name.log"
  fi
  du -sh "$dir/sherpa-onnx-prebuilt" 2>/dev/null || true
  echo
}

mkdir -p "$WORK"
run_case A_network_clean env
run_case B_vendored_archive env "SHERPA_ONNX_ARCHIVE_DIR=$VENDOR"
run_case C_no_network env "ALL_PROXY=socks5://127.0.0.1:9" "HTTPS_PROXY=http://127.0.0.1:9" "HTTP_PROXY=http://127.0.0.1:9"
