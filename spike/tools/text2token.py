#!/usr/bin/env python3
"""ASU-008b spike — sherpa-onnx-cli text2token'in bagimliliksiz esdegeri.

`sentencepiece` / `sherpa-onnx` pip paketleri bu makinede kurulu degil ve spike
kapsaminda paket kurulmuyor. Bu script bpe.model (SentencePiece ModelProto)
dosyasini elle parse edip standart SPM-BPE encode algoritmasini uygular:

  1. metin uppercase + bosluklar U+2581 (LOWER ONE EIGHTH BLOCK) ile isaretlenir
  2. her karakter bir sembol olur
  3. vocab'da bulunan komsu sembol ciftlerinden en yuksek skorlusu birlestirilir
  4. birlestirilecek cift kalmayana kadar tekrarlanir

Dogrulama: modelin kendi keywords_raw.txt -> keywords.txt esleri ile karsilastirilir.
"""

import struct
import sys


def read_varint(buf, i):
    result = 0
    shift = 0
    while True:
        b = buf[i]
        i += 1
        result |= (b & 0x7F) << shift
        if not b & 0x80:
            return result, i
        shift += 7


def parse_pieces(path):
    """ModelProto.pieces (field 1, repeated SentencePiece{piece=1, score=2, type=3})."""
    with open(path, "rb") as f:
        buf = f.read()
    pieces = []
    i = 0
    while i < len(buf):
        key, i = read_varint(buf, i)
        field, wire = key >> 3, key & 7
        if wire == 2:
            length, i = read_varint(buf, i)
            payload = buf[i : i + length]
            i += length
            if field == 1:
                pieces.append(parse_piece(payload))
        elif wire == 0:
            _, i = read_varint(buf, i)
        elif wire == 5:
            i += 4
        elif wire == 1:
            i += 8
        else:
            raise ValueError(f"beklenmeyen wire type {wire}")
    return pieces


def parse_piece(payload):
    piece, score, ptype = "", 0.0, 1
    i = 0
    while i < len(payload):
        key, i = read_varint(payload, i)
        field, wire = key >> 3, key & 7
        if wire == 2:
            length, i = read_varint(payload, i)
            value = payload[i : i + length]
            i += length
            if field == 1:
                piece = value.decode("utf-8")
        elif wire == 5:
            if field == 2:
                score = struct.unpack("<f", payload[i : i + 4])[0]
            i += 4
        elif wire == 0:
            value, i = read_varint(payload, i)
            if field == 3:
                ptype = value
        elif wire == 1:
            i += 8
        else:
            raise ValueError(f"beklenmeyen wire type {wire}")
    return piece, score, ptype


class BpeEncoder:
    def __init__(self, model_path):
        self.pieces = parse_pieces(model_path)
        self.vocab = {}
        for idx, (piece, score, ptype) in enumerate(self.pieces):
            if ptype != 1:  # NORMAL disindakiler (unk/bos/eos/control) atlanir
                continue
            if piece not in self.vocab:
                self.vocab[piece] = (score, idx)
        self.max_len = max(len(p) for p in self.vocab)

    def encode(self, text):
        """SentencePiece UNIGRAM Viterbi cozumu.

        NOT: dosya adi `bpe.model` olmasina ragmen model_type=UNIGRAM (icefall
        `unigram_500`). Dolayisiyla encode = toplam skoru maksimize eden
        segmentasyon, BPE merge degil.
        """
        text = text.strip().upper()
        seq = "▁" + text.replace(" ", "▁")
        n = len(seq)
        unk = min(s for s, _ in self.vocab.values()) - 10.0
        # best[i] = (skor, baslangic_indeksi, parca)
        best = [None] * (n + 1)
        best[0] = (0.0, -1, "")
        for i in range(1, n + 1):
            for j in range(max(0, i - self.max_len), i):
                if best[j] is None:
                    continue
                piece = seq[j:i]
                entry = self.vocab.get(piece)
                score = entry[0] if entry is not None else (unk if i - j == 1 else None)
                if score is None:
                    continue
                cand = (best[j][0] + score, j, piece)
                if best[i] is None or cand[0] > best[i][0]:
                    best[i] = cand
        out = []
        i = n
        while i > 0:
            score, j, piece = best[i]
            out.append(piece)
            i = j
        out.reverse()
        return out

    def encode_str(self, text):
        return " ".join(self.encode(text))


def main():
    if len(sys.argv) < 3:
        print("kullanim: text2token.py <bpe.model> <raw.txt> [out.txt]", file=sys.stderr)
        return 1
    enc = BpeEncoder(sys.argv[1])
    with open(sys.argv[2], encoding="utf-8") as f:
        lines = [ln.rstrip("\n") for ln in f if ln.strip()]

    out_lines = []
    for line in lines:
        # "HEY ASUNA :2.0 #0.35" -> ekler aynen korunur
        parts = line.split()
        words, extras = [], []
        for p in parts:
            (extras if p[0] in ":#@" else words).append(p)
        tokens = enc.encode_str(" ".join(words))
        out_lines.append(" ".join([tokens] + extras))

    text = "\n".join(out_lines) + "\n"
    if len(sys.argv) > 3:
        with open(sys.argv[3], "w", encoding="utf-8") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
