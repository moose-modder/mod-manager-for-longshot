"""Draws the app's icon (stacked layers, for mods on top of the game, on a dark tile) as a 1024x1024 PNG.
Standard library only.

    python scripts/make_icon.py src-tauri/icons/source.png
    npx tauri icon src-tauri/icons/source.png
"""

import math
import struct
import sys
import zlib

SIZE = 1024
TILE = (24, 34, 43)
GOLD = (233, 185, 73)


def rounded_rect(x, y, x0, y0, x1, y1, r):
    cx, cy = min(max(x, x0 + r), x1 - r), min(max(y, y0 + r), y1 - r)
    return math.hypot(x - cx, y - cy) - r


def capsule(x, y, ax, ay, bx, by, r):
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(x - (ax + t * dx), y - (ay + t * dy)) - r


def diamond(x, y, cx, cy, a, b):
    """A filled rhombus (half width a, half height b): the distance to its edges near them."""
    return (abs(x - cx) / a + abs(y - cy) / b - 1) * a * b / math.hypot(a, b)


def chevron(x, y, top, r):
    """A V parallel to the diamond's lower edges, its arms' ends at height top."""
    return min(capsule(x, y, 222, top, 512, top + 145, r), capsule(x, y, 512, top + 145, 802, top, r))


def layers(x, y):
    return min(diamond(x, y, 512, 345, 300, 150), chevron(x, y, 520, 36), chevron(x, y, 665, 36))


def coverage(d):
    return max(0.0, min(1.0, 0.5 - d))


def main(out):
    rows = []
    for y in range(SIZE):
        row = bytearray([0])
        for x in range(SIZE):
            px, py = x + 0.5, y + 0.5
            tile = coverage(rounded_rect(px, py, 48, 48, 976, 976, 210))
            g = coverage(layers(px, py)) * tile
            rgb = [TILE[c] * (1 - g) + GOLD[c] * g for c in range(3)]
            row += bytes([round(v) for v in rgb] + [round(255 * tile)])
        rows.append(bytes(row))
    raw = zlib.compress(b"".join(rows), 9)

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
           + chunk(b"IDAT", raw) + chunk(b"IEND", b""))
    with open(out, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    main(sys.argv[1])
