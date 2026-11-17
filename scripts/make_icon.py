#!/usr/bin/env python3
"""Ritar Ruppas programikon: R-maskoten (blockgrafik, vita ögon, leende) på en rundad mörk ruta.
Skapar assets/icon/ruppa.png (1024), ruppa.ico (Windows), ruppa.icns (macOS, kräver iconutil)
och ruppa-256.rgba (fönsterikonen, inbakad i programmet). Bara standardbiblioteket.

    python3 scripts/make_icon.py
"""
import math
import shutil
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "assets" / "icon"

ART = ["████████▄", "██{e}██", "██{m}██", "████████▀", "██   ▀██▄", "▀▀     ▀▀"]
SHADE = [209, 208, 202, 166, 130, 94]


def xterm(i):
    i -= 16
    lv = [0, 95, 135, 175, 215, 255]
    return (lv[i // 36], lv[(i // 6) % 6], lv[i % 6])


def render(n):
    """RGBA-bytes, n×n. Kantutjämning via 4×4 översampling i kanterna."""
    px = bytearray(n * n * 4)

    # bakgrund: rundad ruta med svag vertikal gradient
    r = n * 0.225
    m = n * 0.06  # marginal runt rutan (som macOS-ikoner)
    x0, y0, x1, y1 = m, m, n - m, n - m

    def in_round(x, y):
        cx = min(max(x, x0 + r), x1 - r)
        cy = min(max(y, y0 + r), y1 - r)
        return (x - cx) ** 2 + (y - cy) ** 2 <= r * r and x0 <= x <= x1 and y0 <= y <= y1

    # figuren: celler i terminalformat (höga celler)
    cw = (x1 - x0) * 0.66 / 9
    ch = cw * 1.42
    fw, fh = 9 * cw, 6 * ch
    fx = (n - fw) / 2
    fy = (n - fh) / 2 + n * 0.01
    rects = []  # (x0, y0, x1, y1, rgb)
    for row, line in enumerate(ART):
        line = line.replace("{e}", "     ").replace("{m}", "     ")
        col = xterm(SHADE[row])
        for c, chr_ in enumerate(line):
            X, Y = fx + c * cw, fy + row * ch
            if chr_ == "█":
                rects.append((X, Y, X + cw, Y + ch, col))
            elif chr_ == "▄":
                rects.append((X, Y + ch / 2, X + cw, Y + ch, col))
            elif chr_ == "▀":
                rects.append((X, Y, X + cw, Y + ch / 2, col))
    # ögon (kolumn 3 och 5, rad 1) och mun (kolumn 4, rad 2)
    eyes = [(fx + 3.5 * cw, fy + 1.5 * ch), (fx + 5.5 * cw, fy + 1.5 * ch)]
    erx, ery = cw * 0.42, ch * 0.28
    pup = cw * 0.22
    mouth_c = (fx + 4.5 * cw, fy + 2.3 * ch)
    mw, md, mt = cw * 0.62, ch * 0.22, cw * 0.11

    def color_at(x, y):
        if not in_round(x, y):
            return None
        t = (y - y0) / (y1 - y0)
        bg = (int(0x1d - 0x10 * t), int(0x22 - 0x12 * t), int(0x2e - 0x16 * t))
        c = bg
        for a, b, cc, d, rgb in rects:
            if a <= x < cc and b <= y < d:
                c = rgb
                break
        for ex, ey in eyes:
            if ((x - ex) / erx) ** 2 + ((y - ey) / ery) ** 2 <= 1:
                c = (255, 255, 255)
                if (x - ex - pup * 0.3) ** 2 + (y - ey - pup * 0.3) ** 2 <= pup * pup:
                    c = (6, 16, 31)
        u = (x - mouth_c[0]) / mw
        if -1 <= u <= 1:
            yy = mouth_c[1] + md * (1 - u * u)
            if abs(y - yy) <= mt:
                c = (255, 255, 255)
        return c

    def edge(x, y):
        """Är pixeln nära en kant? Då översamplas den."""
        s = color_at(x + 0.5, y + 0.5)
        for dx, dy in ((0, 0), (1, 0), (0, 1), (1, 1)):
            if color_at(x + dx, y + dy) != s:
                return True
        return False

    for y in range(n):
        for x in range(n):
            if edge(x, y):
                acc = [0, 0, 0, 0]
                for sy in range(4):
                    for sx in range(4):
                        c = color_at(x + (sx + 0.5) / 4, y + (sy + 0.5) / 4)
                        if c:
                            acc[0] += c[0]
                            acc[1] += c[1]
                            acc[2] += c[2]
                            acc[3] += 1
                a = acc[3]
                rgba = (acc[0] // a, acc[1] // a, acc[2] // a, a * 255 // 16) if a else (0, 0, 0, 0)
            else:
                c = color_at(x + 0.5, y + 0.5)
                rgba = (*c, 255) if c else (0, 0, 0, 0)
            i = (y * n + x) * 4
            px[i : i + 4] = bytes(rgba)
    return bytes(px)


def png(n, rgba):
    raw = b"".join(b"\x00" + rgba[y * n * 4 : (y + 1) * n * 4] for y in range(n))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def downscale(src, n, m):
    """Box-filter från n×n till m×m (n delbart med m), premultiplicerad alfa."""
    f = n // m
    out = bytearray(m * m * 4)
    for y in range(m):
        for x in range(m):
            r = g = b = a = 0
            for dy in range(f):
                for dx in range(f):
                    i = ((y * f + dy) * n + x * f + dx) * 4
                    al = src[i + 3]
                    r += src[i] * al
                    g += src[i + 1] * al
                    b += src[i + 2] * al
                    a += al
            j = (y * m + x) * 4
            out[j : j + 4] = bytes((r // a, g // a, b // a, a // (f * f))) if a else b"\x00\x00\x00\x00"
    return bytes(out)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    big = render(1024)
    (OUT / "ruppa.png").write_bytes(png(1024, big))
    sizes = {s: downscale(big, 1024, s) for s in (512, 256, 128, 64, 32, 16)}
    sizes[48] = render(48)
    (OUT / "ruppa-256.rgba").write_bytes(sizes[256])
    # .ico med PNG-poster
    order = [16, 32, 48, 64, 128, 256]
    pngs = [png(s, sizes[s]) for s in order]
    head = struct.pack("<HHH", 0, 1, len(order))
    off = 6 + 16 * len(order)
    entries = b""
    for s, data in zip(order, pngs):
        entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), off)
        off += len(data)
    (OUT / "ruppa.ico").write_bytes(head + entries + b"".join(pngs))
    # .icns via iconutil (macOS)
    if shutil.which("iconutil"):
        with tempfile.TemporaryDirectory() as td:
            iset = Path(td) / "ruppa.iconset"
            iset.mkdir()
            for s in (16, 32, 128, 256, 512):
                (iset / f"icon_{s}x{s}.png").write_bytes(png(s, sizes[s]))
                s2 = s * 2
                data = big if s2 == 1024 else sizes[s2]
                (iset / f"icon_{s}x{s}@2x.png").write_bytes(png(s2, data))
            subprocess.run(["iconutil", "-c", "icns", str(iset), "-o", str(OUT / "ruppa.icns")], check=True)
    for f in sorted(OUT.iterdir()):
        print("skrev", f.relative_to(OUT.parent.parent), f.stat().st_size, "byte")


if __name__ == "__main__":
    main()
