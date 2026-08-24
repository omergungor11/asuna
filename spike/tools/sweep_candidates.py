#!/usr/bin/env python3
"""ASU-008b — aday keyword yazimlarini tek tek tarar.

ASR tanilamasi gosterdi ki model "Hey Asuna"yi ortografik degil fonetik olarak
decode ediyor ("HEY AS SOONER"). Bu script her aday yazimi ayri ayri korpustan
gecirip detection / false-accept sayar.
"""
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from text2token import BpeEncoder  # noqa: E402

ROOT = os.path.dirname(HERE)
MODEL = os.path.join(ROOT, "models", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01")
BIN = os.path.join(ROOT, "kws", "target", "release", "kws-batch")
AUDIO = os.path.join(ROOT, "audio")
GROUPS = ["pos_en", "pos_tr", "neg", "amb"]

CANDIDATES = [
    # ASR ciktisindan veri-odakli turetilen birlesim
    ["HEY AS SOON", "HEY AS SOONER", "HEY SOONER", "HEY SOON", "AS SOONER", "A SOONER"],
    ["HEY AS SOON", "HEY AS SOONER", "HEY SOONER"],
    ["HEY AS SOON", "HEY ASUNA"],
    ["HEY ASUNA"],
    ["HEY AS SOONER"],
    ["HEY AS SOON A"],
    ["HEY AS SOON"],
    ["HEY SOONER"],
    ["HEY ASSUMED"],
    ["HEY A SOONER"],
    ["HEY AS SOON ARE"],
    # birlesik setler (uretimde birden fazla yazim ayni wake word'e baglanir)
    ["HEY AS SOONER", "HEY AS SOON A"],
    ["HEY AS SOONER", "HEY AS SOON A", "HEY ASUNA"],
    ["HEY AS SOONER", "HEY AS SOON A", "HEY ASUNA", "HEY ASSUMED", "HEY SOONER"],
]

enc = BpeEncoder(os.path.join(MODEL, "bpe.model"))


def run(kw_path, score, thr, group):
    out = subprocess.run(
        [BIN, MODEL, kw_path, str(score), str(thr), os.path.join(AUDIO, group)],
        capture_output=True,
        text=True,
        check=True,
    )
    total = hit = 0
    for line in out.stdout.splitlines():
        c = line.split("\t")
        if len(c) < 3:
            continue
        total += 1
        hit += int(c[1])
    return total, hit


def main():
    score = float(os.environ.get("SCORE", "1.0"))
    thresholds = [float(x) for x in os.environ.get("THRESHOLDS", "0.05,0.15,0.25,0.35,0.45").split(",")]
    print(f"# boosting score = {score}")
    print("thr\tpos_en\tpos_tr\tneg_FA\tamb\tpos_rate\tkeyword")
    for cand in CANDIDATES:
        with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False, encoding="utf-8") as f:
            f.write("\n".join(enc.encode_str(c) for c in cand) + "\n")
            kw_path = f.name
        for thr in thresholds:
            res = {g: run(kw_path, score, thr, g) for g in GROUPS}
            pos_hit = res["pos_en"][1] + res["pos_tr"][1]
            pos_n = res["pos_en"][0] + res["pos_tr"][0]
            print(
                "%.2f\t%d/%d\t%d/%d\t%d/%d\t%d/%d\t%.0f%%\t%s"
                % (
                    thr,
                    res["pos_en"][1], res["pos_en"][0],
                    res["pos_tr"][1], res["pos_tr"][0],
                    res["neg"][1], res["neg"][0],
                    res["amb"][1], res["amb"][0],
                    100.0 * pos_hit / pos_n,
                    " + ".join(cand),
                ),
                flush=True,
            )
        os.unlink(kw_path)
        print()


if __name__ == "__main__":
    main()
