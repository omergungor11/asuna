#!/usr/bin/env python3
"""ASU-008b — test korpusunun sure istatistigi (FA oraninin baglami icin)."""
import os
import wave

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AUDIO = os.path.join(ROOT, "audio")

total_all = 0.0
for group in sorted(os.listdir(AUDIO)):
    gdir = os.path.join(AUDIO, group)
    if not os.path.isdir(gdir):
        continue
    total = 0.0
    n = 0
    for f in sorted(os.listdir(gdir)):
        if not f.endswith(".wav"):
            continue
        with wave.open(os.path.join(gdir, f)) as w:
            total += w.getnframes() / w.getframerate()
            n += 1
    total_all += total
    print(f"{group:8s} n={n:3d}  toplam={total:6.1f} s  ortalama={total / max(n, 1):.2f} s")
print(f"{'TOPLAM':8s}        toplam={total_all:6.1f} s ({total_all / 60:.1f} dk)")
