#!/usr/bin/env python3
"""Render a dither-ratatui gallery snapshot (JSON cell dump) to a PNG.

    cargo run -p dither-ratatui --example gallery -- --snapshot out --raster braille
    python3 scripts/render_snapshot.py out/charts.json out/charts.png

Draws each cell as a terminal would: background fill, block elements drawn
pixel-exact, other glyphs (braille included) with a monospace font, and kitty
graphics decoded from the APC transmissions and placed by their Unicode
placeholder diacritics. Needs Pillow and a monospace font with braille
(DejaVu Sans Mono by default; override with SNAPSHOT_FONT).
"""
import base64, json, os, re, sys, zlib
from PIL import Image, ImageDraw, ImageFont

BG = (11, 11, 15)
FG = (231, 231, 234)
FONT = os.environ.get("SNAPSHOT_FONT", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
FONT_BOLD = FONT.replace("DejaVuSansMono.ttf", "DejaVuSansMono-Bold.ttf")
KITTY_RS = os.path.join(os.path.dirname(__file__), "..", "crates", "dither-ratatui", "src", "kitty.rs")


def diacritics():
    src = open(KITTY_RS).read()
    table = src[src.index("static DIACRITICS"):]
    table = table[table.index("[", table.index("=")) + 1:table.index("];")]
    return {int(h, 16): i for i, h in enumerate(re.findall(r"0x([0-9A-Fa-f]+)", table))}


def main(src, dst):
    d = json.load(open(src))
    cols, rows = d["cols"], d["rows"]
    cw, ch = d["cell"]
    img = Image.new("RGB", (cols * cw, rows * ch), BG)
    draw = ImageDraw.Draw(img)
    size = int(ch * 0.78)
    font = ImageFont.truetype(FONT, size)
    bold = ImageFont.truetype(FONT_BOLD, size) if os.path.exists(FONT_BOLD) else font
    diac = diacritics()
    images, apc = {}, []
    placeholders = []
    for i, (sym, fg, bg, is_bold) in enumerate(d["cells"]):
        x, y = (i % cols) * cw, (i // cols) * ch
        bgc = tuple(bg) if bg else BG
        fgc = tuple(fg) if fg else FG
        draw.rectangle([x, y, x + cw - 1, y + ch - 1], fill=bgc)
        # Kitty graphics: collect transmissions, remember placeholder cells.
        for m in re.finditer(r"\x1b_G([^;]*);([^\x1b]*)\x1b\\", sym):
            apc.append((dict(kv.split("=") for kv in m.group(1).split(",")), m.group(2)))
            if apc[-1][0].get("m", "0") == "0":
                keys = apc[0][0]
                raw = zlib.decompress(base64.b64decode("".join(p for _, p in apc)))
                w, h = int(keys["s"]), int(keys["v"])
                images[int(keys["i"])] = Image.frombytes("RGBA", (w, h), raw)
                apc = []
        text = re.sub(r"\x1b_G[^\x1b]*\x1b\\", "", sym)
        if text.startswith("\U0010EEEE"):
            r, c = diac[ord(text[1])], diac[ord(text[2])]
            placeholders.append((x, y, (fg[0] << 16) | (fg[1] << 8) | fg[2], r, c))
            continue
        if text == "▀":
            draw.rectangle([x, y, x + cw - 1, y + ch // 2 - 1], fill=fgc)
        elif text == "▄":
            draw.rectangle([x, y + ch // 2, x + cw - 1, y + ch - 1], fill=fgc)
        elif len(text) == 1 and 0x2800 < ord(text) <= 0x28FF:
            # Braille, drawn as terminals like kitty and Ghostty draw it.
            bits = ord(text) - 0x2800
            order = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (0, 3), (1, 3)]
            r = cw * 0.17
            for b, (dx, dy) in enumerate(order):
                if bits >> b & 1:
                    cx, cy = x + (dx + 0.5) * cw / 2, y + (dy + 0.5) * ch / 4
                    draw.ellipse([cx - r, cy - r, cx + r, cy + r], fill=fgc)
        elif text.strip():
            draw.text((x, y + ch * 0.06), text, font=bold if is_bold else font, fill=fgc)
    for x, y, iid, r, c in placeholders:
        im = images.get(iid)
        if im:
            tile = im.crop((c * cw, r * ch, (c + 1) * cw, (r + 1) * ch))
            img.paste(tile, (x, y), tile)
    img.save(dst)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
