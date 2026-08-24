#!/usr/bin/env python3
"""ASU-008b — uzun negatif akista false-accept olcumu (FA / saat).

`kws-batch` her dosya icin toplam tetiklenme sayisini (3. kolon) basar;
burada dosya sureleriyle normalize edilip saatlik FA'ya cevrilir.
"""
import os
import subprocess
import sys
import tempfile
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from text2token import BpeEncoder  # noqa: E402

ROOT = os.path.dirname(HERE)
MODEL = os.path.join(ROOT, "models", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01")
BIN = os.path.join(ROOT, "kws", "target", "release", "kws-batch")
LONG = os.path.join(ROOT, "audio_long")

CONFIGS = [
    # (etiket, keyword listesi, boosting score, threshold)
    ("ortografik", ["HEY ASUNA"], 1.0, 0.25),
    ("ortografik-agresif", ["HEY ASUNA"], 4.0, 0.05),
    ("fonetik-tek", ["HEY AS SOON"], 2.5, 0.15),
    ("fonetik-tek-muhafazakar", ["HEY AS SOON"], 1.0, 0.25),
    (
        "fonetik-genis",
        ["HEY AS SOON", "HEY AS SOONER", "HEY SOONER", "HEY SOON", "AS SOONER", "A SOONER"],
        2.5,
        0.05,
    ),
    (
        "fonetik-orta",
        ["HEY AS SOON", "HEY AS SOONER", "HEY SOONER"],
        2.5,
        0.05,
    ),
]

enc = BpeEncoder(os.path.join(MODEL, "bpe.model"))

total_seconds = 0.0
for f in sorted(os.listdir(LONG)):
    if f.endswith(".wav"):
        with wave.open(os.path.join(LONG, f)) as w:
            total_seconds += w.getnframes() / w.getframerate()
print(f"# uzun negatif akis: {total_seconds:.0f} s ({total_seconds / 60:.1f} dk)")
print("etiket\tscore\tthr\ttetiklenme\tFA/saat\tkeywords")

for label, kws, score, thr in CONFIGS:
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8") as f:
        f.write("\n".join(enc.encode_str(k) for k in kws) + "\n")
        path = f.name
    out = subprocess.run(
        [BIN, MODEL, path, str(score), str(thr), LONG],
        capture_output=True,
        text=True,
        check=True,
    )
    fires = 0
    for line in out.stdout.splitlines():
        c = line.split("\t")
        if len(c) >= 3:
            fires += int(c[2])
    os.unlink(path)
    print(
        "%s\t%.1f\t%.2f\t%d\t%.1f\t%s"
        % (label, score, thr, fires, fires / (total_seconds / 3600.0), " + ".join(kws))
    )
