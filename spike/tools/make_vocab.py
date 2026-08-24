#!/usr/bin/env python3
"""bpe.model -> bpe.vocab (sentencepiece `--export_vocab` bicimi: piece<TAB>score)."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from text2token import parse_pieces  # noqa: E402

MODEL = os.path.join(
    os.path.dirname(HERE), "models", "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01"
)
pieces = parse_pieces(os.path.join(MODEL, "bpe.model"))
out = os.path.join(MODEL, "bpe.vocab")
with open(out, "w", encoding="utf-8") as f:
    for piece, score, _ in pieces:
        f.write(f"{piece}\t{score}\n")
print(f"{out} yazildi, {len(pieces)} parca")
