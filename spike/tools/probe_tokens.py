#!/usr/bin/env python3
"""ASU-008b — "HEY ASUNA" ve fonetik komsularinin unigram tokenizasyonu."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from text2token import BpeEncoder  # noqa: E402

MODEL = os.path.join(
    os.path.dirname(HERE),
    "models",
    "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01",
)

PHRASES = [
    "HEY ASUNA",
    "ASUNA",
    "HEY ASUNA CHAN",
    "HEY ASOONA",
    "HEY AH SOO NA",
    "OK ASUNA",
    "HEY ALEXA",
    "HEY SIRI",
    "HESABINA",
    "ASANSOR",
    "HEY ASSUNA",
    "A SUNA",
    "HEY ASUNAH",
    "HEY AZUNA",
    "HEY OSUNA",
    "HEY A SUNA",
]

enc = BpeEncoder(os.path.join(MODEL, "bpe.model"))
for p in PHRASES:
    print("%-16s -> %s" % (p, enc.encode_str(p)))

print()
probe = ["▁ASUNA", "▁AS", "UN", "UNA", "NA", "A", "▁HE", "Y", "SU", "▁A", "S", "U", "N", "▁HEY", "HEY"]
print("vocab uyeligi:")
for w in probe:
    print("  %-8s %s" % (w, "VAR" if w in enc.vocab else "YOK"))
print()
print("vocab boyutu (NORMAL parcalar):", len(enc.vocab))
