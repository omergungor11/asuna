#!/usr/bin/env python3
"""ASU-008b — nihai tablo: her konfigurasyon icin detection + FA/saat."""
import os
import subprocess
import sys
import tempfile
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from fa_long import CONFIGS  # noqa: E402
from text2token import BpeEncoder  # noqa: E402

ROOT = os.path.dirname(HERE)
MODEL = os.path.join(ROOT, "models", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01")
BIN = os.path.join(ROOT, "kws", "target", "release", "kws-batch")
AUDIO = os.path.join(ROOT, "audio")
LONG = os.path.join(ROOT, "audio_long")

enc = BpeEncoder(os.path.join(MODEL, "bpe.model"))

long_seconds = 0.0
for f in os.listdir(LONG):
    if f.endswith(".wav"):
        with wave.open(os.path.join(LONG, f)) as w:
            long_seconds += w.getnframes() / w.getframerate()


def run(path, score, thr, target):
    out = subprocess.run(
        [BIN, MODEL, path, str(score), str(thr), target],
        capture_output=True,
        text=True,
        check=True,
    )
    n = hit = fires = 0
    for line in out.stdout.splitlines():
        c = line.split("\t")
        if len(c) < 3:
            continue
        n += 1
        hit += int(c[1])
        fires += int(c[2])
    return n, hit, fires


print("config\tscore\tthr\tpos_en\tpos_tr\tdet%\tneg42s\tFA/saat")
for label, kws, score, thr in CONFIGS:
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8") as f:
        f.write("\n".join(enc.encode_str(k) for k in kws) + "\n")
        path = f.name
    en_n, en_hit, _ = run(path, score, thr, os.path.join(AUDIO, "pos_en"))
    tr_n, tr_hit, _ = run(path, score, thr, os.path.join(AUDIO, "pos_tr"))
    _, neg_hit, _ = run(path, score, thr, os.path.join(AUDIO, "neg"))
    _, _, long_fires = run(path, score, thr, LONG)
    os.unlink(path)
    print(
        "%s\t%.1f\t%.2f\t%d/%d\t%d/%d\t%.0f%%\t%d\t%.1f"
        % (
            label,
            score,
            thr,
            en_hit,
            en_n,
            tr_hit,
            tr_n,
            100.0 * (en_hit + tr_hit) / (en_n + tr_n),
            neg_hit,
            long_fires / (long_seconds / 3600.0),
        )
    )
