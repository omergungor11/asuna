#!/usr/bin/env python3
"""ASU-008b — threshold x boosting-score taramasi.

Her kombinasyon icin kws-batch calistirir, grup bazinda
detection / false-accept sayar ve TSV tablo basar.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODEL = os.path.join(ROOT, "models", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01")
BIN = os.path.join(ROOT, "kws", "target", "release", "kws-batch")
AUDIO = os.path.join(ROOT, "audio")
GROUPS = ["pos_en", "pos_tr", "neg", "amb"]

THRESHOLDS = [float(x) for x in os.environ.get("THRESHOLDS", "0.05,0.15,0.25,0.35,0.45").split(",")]
SCORES = [float(x) for x in os.environ.get("SCORES", "0.5,1.0,1.5,2.0,3.0,4.0").split(",")]


def count_dir(keywords, score, threshold, group):
    out = subprocess.run(
        [BIN, MODEL, keywords, str(score), str(threshold), os.path.join(AUDIO, group)],
        capture_output=True,
        text=True,
        check=True,
    )
    total = 0
    hit = 0
    fires = 0
    for line in out.stdout.splitlines():
        cols = line.split("\t")
        if len(cols) < 3:
            continue
        total += 1
        hit += int(cols[1])
        fires += int(cols[2])
    return total, hit, fires


def main():
    keywords = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "keywords", "keywords_asuna.txt")
    with open(keywords, encoding="utf-8") as f:
        print("# keywords:", f.read().strip().replace("\n", " | "))
    header = ["thr", "score"]
    for g in GROUPS:
        header += [f"{g}_hit", f"{g}_n", f"{g}_fires"]
    header.append("pos_rate")
    print("\t".join(header))
    for thr in THRESHOLDS:
        for sc in SCORES:
            row = [f"{thr:.2f}", f"{sc:.1f}"]
            pos_hit = pos_n = 0
            for g in GROUPS:
                total, hit, fires = count_dir(keywords, sc, thr, g)
                row += [str(hit), str(total), str(fires)]
                if g.startswith("pos"):
                    pos_hit += hit
                    pos_n += total
            row.append(f"{100.0 * pos_hit / max(pos_n, 1):.1f}%")
            print("\t".join(row), flush=True)


if __name__ == "__main__":
    main()
