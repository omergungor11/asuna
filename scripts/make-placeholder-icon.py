"""Asuna placeholder app icon uretici (bagimliliksiz PNG yazici).

Tauri sablonunun kendi logosunu urune gomemek icin, tasarim gelene kadar
kullanilacak notr bir placeholder uretir: koyu zemin + acik halka.
Cikti 1024x1024 RGBA PNG; `pnpm tauri icon` bundan tum boyutlari turetir.
"""

import math
import struct
import sys
import zlib

SIZE = 1024
BG = (13, 15, 20, 255)  # #0d0f14 — app.css ile ayni
FG = (230, 232, 238, 255)  # #e6e8ee

CENTER = (SIZE - 1) / 2.0
RING_OUTER = SIZE * 0.34
RING_INNER = SIZE * 0.26
DOT_RADIUS = SIZE * 0.075
AA = 1.5  # kenar yumusatma bandi (piksel)


def coverage(distance: float, edge: float, inside: bool) -> float:
    """Kenardan uzakliga gore 0..1 kapsama (basit analitik antialiasing)."""
    delta = (edge - distance) if inside else (distance - edge)
    return max(0.0, min(1.0, delta / AA + 0.5))


def blend(bg, fg, alpha):
    return tuple(round(b + (f - b) * alpha) for b, f in zip(bg[:3], fg[:3])) + (255,)


def build_rows():
    rows = []
    for y in range(SIZE):
        dy = y - CENTER
        row = bytearray()
        for x in range(SIZE):
            dx = x - CENTER
            dist = math.hypot(dx, dy)

            ring = min(coverage(dist, RING_OUTER, True), coverage(dist, RING_INNER, False))
            dot = coverage(dist, DOT_RADIUS, True)
            alpha = max(ring, dot)

            row.extend(blend(BG, FG, alpha) if alpha > 0 else BG)
        rows.append(bytes(row))
    return rows


def chunk(tag: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + tag
        + payload
        + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
    )


def main(path: str) -> None:
    raw = b"".join(b"\x00" + row for row in build_rows())
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as handle:
        handle.write(png)
    print(f"wrote {path} ({len(png)} bytes)")


if __name__ == "__main__":
    main(sys.argv[1])
