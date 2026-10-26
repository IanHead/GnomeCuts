"""Generates gnomecuts.ico: the same 2x2 workspace grid as the tray icon.

Standard library only. Run from anywhere: python assets/make_icon.py
"""

import struct
import zlib
from pathlib import Path

SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
BLUE = (0x35, 0x84, 0xE4)
GREY = (0x9A, 0x99, 0x96)
SUPERSAMPLE = 4


def draw(size):
    """Returns size*size RGBA pixels, antialiased by supersampling."""
    pad = max(1, size / 16)
    gap = max(1, size / 8)
    tile = (size - 2 * pad - gap) / 2
    radius = tile * 0.18 if size >= 24 else 0

    tiles = []
    for row in (0, 1):
        for col in (0, 1):
            x0, y0 = pad + col * (tile + gap), pad + row * (tile + gap)
            tiles.append((x0, y0, BLUE if (row, col) == (0, 0) else GREY))

    def inside(px, py):
        for x0, y0, colour in tiles:
            if not (x0 <= px < x0 + tile and y0 <= py < y0 + tile):
                continue
            # Distance into the rounded corner, if any.
            dx = max(x0 + radius - px, px - (x0 + tile - radius), 0)
            dy = max(y0 + radius - py, py - (y0 + tile - radius), 0)
            if dx * dx + dy * dy <= radius * radius:
                return colour
        return None

    pixels = []
    n = SUPERSAMPLE
    for y in range(size):
        for x in range(size):
            hits = []
            for sy in range(n):
                for sx in range(n):
                    c = inside(x + (sx + 0.5) / n, y + (sy + 0.5) / n)
                    if c:
                        hits.append(c)
            if not hits:
                pixels.append((0, 0, 0, 0))
                continue
            # Tiles never overlap, so all hits in a pixel share one colour.
            r, g, b = hits[0]
            pixels.append((r, g, b, round(255 * len(hits) / (n * n))))
    return pixels


def png(size, pixels):
    rows = b"".join(
        b"\x00" + bytes(v for p in pixels[y * size:(y + 1) * size] for v in p)
        for y in range(size)
    )

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")


def dib(size, pixels):
    """Uncompressed 32-bit BMP frame, readable by every icon consumer."""
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    # Bottom-up BGRA rows, then an all-zero AND mask (alpha decides transparency).
    colour = b"".join(
        bytes((b, g, r, a)) for y in reversed(range(size)) for r, g, b, a in pixels[y * size:(y + 1) * size]
    )
    mask = bytes(((size + 31) // 32 * 4) * size)
    return header + colour + mask


def main():
    # Windows convention: bitmaps for small sizes, PNG only for 256 px.
    images = [png(s, draw(s)) if s == 256 else dib(s, draw(s)) for s in SIZES]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in zip(SIZES, images):
        dim = 0 if size == 256 else size  # 0 means 256 in the ICO directory
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    out += b"".join(images)
    path = Path(__file__).with_name("gnomecuts.ico")
    path.write_bytes(out)
    print(f"Wrote {path} ({len(out):,} bytes)")


if __name__ == "__main__":
    main()
