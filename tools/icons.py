"""Draw our own map icons (weapon, armor, rune) as PNGs in icons/, and write each as the body of a
Flash DefineBitsLossless2 tag (without its character id) to plugin/src/icons/<name>.bin, which the
plugin adds to the world map at startup (see plugin/src/gfx.rs).

Each is a dark round seal with a coloured rim and a light glyph, the same footprint as the map's
own round icons (87/88). Drawn at 4x and scaled down for smooth edges.

    python tools/icons.py      (also run by generate.py)
"""
import math
import os
import struct
import zlib

from PIL import Image, ImageDraw, ImageFilter

import common

SIZE = 128          # final bitmap size; generate.py places it at ~72 px on the map
SS = 4              # supersampling
ICON_DIR = os.path.join(common.ROOT, "icons")
BIN_DIR = os.path.join(common.ROOT, "plugin", "src", "icons")

# name: (rim colour, glyph colour)
STYLE = {
    "weapon": ((214, 120, 52), (240, 228, 205)),
    "armor": ((150, 168, 186), (240, 236, 226)),
    "rune": ((92, 170, 230), (214, 240, 255)),
}


def seal(rim):
    """Dark disc, gold-ish ring in the rim colour, soft outer glow."""
    n = SIZE * SS
    im = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    glow = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse((n * 0.06, n * 0.06, n * 0.94, n * 0.94), fill=rim + (150,))
    im.alpha_composite(glow.filter(ImageFilter.GaussianBlur(n * 0.035)))
    d = ImageDraw.Draw(im)
    d.ellipse((n * 0.11, n * 0.11, n * 0.89, n * 0.89), fill=(28, 26, 24, 255), outline=rim + (255,),
              width=int(n * 0.045))
    d.ellipse((n * 0.17, n * 0.17, n * 0.83, n * 0.83), outline=rim + (110,), width=int(n * 0.012))
    return im, d


def poly(d, pts, fill, n):
    d.polygon([(x * n, y * n) for x, y in pts], fill=fill)


def rotate(pts, deg, cx=0.5, cy=0.5):
    a = math.radians(deg)
    return [(cx + (x - cx) * math.cos(a) - (y - cy) * math.sin(a),
             cy + (x - cx) * math.sin(a) + (y - cy) * math.cos(a)) for x, y in pts]


def weapon(d, c, n):
    """A straight sword, point up-right."""
    blade = [(0.47, 0.30), (0.50, 0.25), (0.53, 0.30), (0.53, 0.60), (0.47, 0.60)]
    guard = [(0.38, 0.60), (0.62, 0.60), (0.62, 0.645), (0.38, 0.645)]
    grip = [(0.483, 0.645), (0.517, 0.645), (0.517, 0.73), (0.483, 0.73)]
    pommel = [(0.47, 0.73), (0.53, 0.73), (0.53, 0.765), (0.47, 0.765)]
    for part in (blade, guard, grip, pommel):
        poly(d, rotate(part, 45), c, n)
    poly(d, rotate([(0.497, 0.31), (0.503, 0.31), (0.503, 0.58), (0.497, 0.58)], 45), (28, 26, 24, 255), n)


def armor(d, c, n):
    """A great helm: rounded top, eye slit, breathing holes."""
    d.rounded_rectangle((0.33 * n, 0.27 * n, 0.67 * n, 0.72 * n), radius=0.15 * n, fill=c)
    d.rectangle((0.33 * n, 0.50 * n, 0.67 * n, 0.72 * n), fill=c)
    dark = (28, 26, 24, 255)
    d.rectangle((0.37 * n, 0.45 * n, 0.63 * n, 0.49 * n), fill=dark)       # eye slit
    d.rectangle((0.485 * n, 0.30 * n, 0.515 * n, 0.45 * n), fill=dark)     # crest ridge
    for i in range(3):
        for j in range(2):
            x, y = 0.43 + i * 0.07, 0.56 + j * 0.06
            d.ellipse(((x - 0.012) * n, (y - 0.012) * n, (x + 0.012) * n, (y + 0.012) * n), fill=dark)


def rune(d, c, n):
    """An angular rune: a stave with branches and a ring."""
    w = int(n * 0.045)
    lines = [((0.50, 0.26), (0.50, 0.74)),
             ((0.50, 0.38), (0.36, 0.28)), ((0.50, 0.38), (0.64, 0.28)),
             ((0.50, 0.58), (0.37, 0.70)), ((0.50, 0.58), (0.63, 0.70))]
    for (x1, y1), (x2, y2) in lines:
        d.line((x1 * n, y1 * n, x2 * n, y2 * n), fill=c, width=w)
    r = 0.075
    d.ellipse(((0.5 - r) * n, (0.48 - r) * n, (0.5 + r) * n, (0.48 + r) * n), outline=c, width=w)


GLYPHS = {"weapon": weapon, "armor": armor, "rune": rune}


def draw(name):
    rim, glyph = STYLE[name]
    im, d = seal(rim)
    GLYPHS[name](d, glyph + (255,), SIZE * SS)
    return im.resize((SIZE, SIZE), Image.LANCZOS)


def lossless2_body(im):
    """DefineBitsLossless2 after the character id: format 5 (32-bit), width, height, then zlib'd
    rows of premultiplied [A, R, G, B]."""
    im = im.convert("RGBA")
    raw, px = im.tobytes(), bytearray()
    for i in range(0, len(raw), 4):
        r, g, b, a = raw[i:i + 4]
        px += bytes((a, r * a // 255, g * a // 255, b * a // 255))
    return struct.pack("<BHH", 5, im.width, im.height) + zlib.compress(bytes(px), 9)


def main():
    os.makedirs(ICON_DIR, exist_ok=True)
    os.makedirs(BIN_DIR, exist_ok=True)
    for name in STYLE:
        im = draw(name)
        im.save(os.path.join(ICON_DIR, name + ".png"))
        open(os.path.join(BIN_DIR, name + ".bin"), "wb").write(lossless2_body(im))
    print(f"Wrote {len(STYLE)} icons to {ICON_DIR} and {BIN_DIR}")


if __name__ == "__main__":
    main()
