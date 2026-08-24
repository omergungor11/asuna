#!/usr/bin/env bash
# ASU-008b — sherpa-onnx 1.13.6 + sherpa-onnx-sys 1.13.6 ciftinin derlenip
# derlenmedigini test eder (1.13.5 wrapper'i sys 1.13.6 ile DERLENMIYOR).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
W="$ROOT/buildtest/v1136"
rm -rf "$W"
mkdir -p "$W/src"
cat > "$W/Cargo.toml" <<'TOML'
[package]
name = "v1136-probe"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
sherpa-onnx = "=1.13.6"

[[bin]]
name = "probe"
path = "src/main.rs"
TOML
cat > "$W/src/main.rs" <<'RS'
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig};
fn main() {
    let c = KeywordSpotterConfig::default();
    println!("{:?}", KeywordSpotter::create(&c).is_some());
}
RS
cd "$W"
start=$SECONDS
cargo build --release > "$W/build.log" 2>&1
rc=$?
echo "1.13.6 cifti: exit=$rc sure=$((SECONDS - start))s"
[ $rc -ne 0 ] && tail -12 "$W/build.log"
grep -E "^sherpa-onnx(-sys)? " "$W/Cargo.lock" 2>/dev/null | head
ls "$W/target/sherpa-onnx-prebuilt" 2>/dev/null
exit 0
