#!/usr/bin/env python3
"""ASU-008b — olcum ozetleyici: min/ortalama/p95/max CPU% ve RSS."""
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MEAS = os.path.join(ROOT, "measurements")


def summarize(path):
    cpus, rss = [], []
    with open(path, encoding="utf-8") as f:
        next(f)
        for line in f:
            c = line.split("\t")
            if len(c) < 3:
                continue
            try:
                cpus.append(float(c[1]))
                rss.append(int(c[2]))
            except ValueError:
                continue
    if not cpus:
        print(f"{os.path.basename(path)}: ornek yok")
        return
    cpus_s = sorted(cpus)
    rss_s = sorted(rss)
    p95 = cpus_s[int(0.95 * (len(cpus_s) - 1))]
    print(
        "%-22s n=%3d  sure=%4ds | CPU%%: ort=%.2f med=%.2f p95=%.2f max=%.2f "
        "| RSS MB: ort=%.1f med=%.1f max=%.1f"
        % (
            os.path.basename(path).replace(".samples.tsv", ""),
            len(cpus),
            5 * len(cpus),
            sum(cpus) / len(cpus),
            cpus_s[len(cpus_s) // 2],
            p95,
            cpus_s[-1],
            sum(rss) / len(rss) / 1024,
            rss_s[len(rss_s) // 2] / 1024,
            rss_s[-1] / 1024,
        )
    )


files = sys.argv[1:] or sorted(
    os.path.join(MEAS, f) for f in os.listdir(MEAS) if f.endswith(".samples.tsv")
)
for f in files:
    summarize(f)
