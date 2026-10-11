"""Export Convergence's class note icons to icons/notes/<Class> (path|weapons).png, as a starting
point for editing. The plugin reads that folder at every start and shows each file next to the
class name in item descriptions (see plugin/src/noteicons.rs); a note without a file keeps
Convergence's icon.

The description box puts an image's bottom on the text baseline and grows the line upward for
anything taller, so the export is cut to sit like a letter: the hanging tail below the paper is
cut off and the empty margins trimmed, then scaled to ART_SHARE of a square, with the paper's
bottom edge on the square's bottom edge, centred across.

The icons come from the menu icon atlas (menu/hi/01_common.tpf + .sblytbnd), the same pixels the
description box draws for MENU_ItemIcon_<id>. Existing files are kept: pass --force to overwrite.

    python tools/note_icons.py [--size 96] [--force]

Any PNG works in game, at any size and shape: 96 image pixels are drawn as 32 screen pixels
(ICON_FILE_PX / ICON_PX in plugin/src/msg.rs), so the default 96 px square is 32 px on screen.
"""
import io
import json
import os
import re
import struct
import sys

from PIL import Image

import common
from dump_fmg import bnd4, dcx

OUT_DIR = os.path.join(common.ROOT, "icons", "notes")
MENU_HI = os.path.join(common.MOD, "menu", "hi")
TAIL_WIDTH = 0.15   # rows narrower than this share of the paper's width are the tail
ART_SHARE = 2 / 3   # the note's widest side, as a share of the square's side


def file_name(cls, which):
    """Must match noteicons::file_name in the plugin."""
    return f"{cls} ({which}).png"


def notes():
    """[(goods id, class, which)] for every class note."""
    out = []
    for c in json.load(open(os.path.join(common.CACHE, "classes.json"), encoding="utf-8")):
        for which, note in (("path", c["pathId"]), ("weapons", c.get("weaponsId"))):
            if note:
                out.append((note, c["name"], which))
    return out


def atlas_icons(wanted):
    """{icon id: RGBA image} for the MENU_ItemIcon_<id> entries in `wanted`."""
    where = {}
    for _, _, b in bnd4(dcx(open(os.path.join(MENU_HI, "01_common.sblytbnd.dcx"), "rb").read())):
        t = b.decode("utf-8")
        atlas = re.search(r'imagePath="([^"]+)\.png"', t).group(1)
        for m in re.finditer(r'name="MENU_ItemIcon_(\d+)\.png" x="(\d+)" y="(\d+)" width="(\d+)" height="(\d+)"', t):
            if int(m.group(1)) in wanted:
                where[int(m.group(1))] = (atlas, *map(int, m.groups()[1:]))

    tpf = dcx(open(os.path.join(MENU_HI, "01_common.tpf.dcx"), "rb").read())
    count = struct.unpack_from("<i", tpf, 0x08)[0]
    textures = {}
    for i in range(count):
        off, size = struct.unpack_from("<Ii", tpf, 0x10 + i * 0x14)
        name_off = struct.unpack_from("<I", tpf, 0x10 + i * 0x14 + 0x0C)[0]
        end = name_off
        while tpf[end:end + 2] != b"\0\0":
            end += 2
        textures[tpf[name_off:end].decode("utf-16-le")] = (off, size)

    decoded, out = {}, {}
    for icon, (atlas, x, y, w, h) in where.items():
        if atlas not in decoded:
            off, size = textures[atlas]
            decoded[atlas] = Image.open(io.BytesIO(tpf[off:off + size])).convert("RGBA")
        out[icon] = decoded[atlas].crop((x, y, x + w, y + h))
    return out


def trim(img):
    """The note without its tail and empty margins. Going down from the paper's widest row, the
    paper ends at the first row narrower than TAIL_WIDTH of it; everything below is the tail."""
    alpha = img.getchannel("A").point(lambda a: 255 if a > 16 else 0)
    widths = [sum(1 for x in range(img.width) if alpha.getpixel((x, y))) for y in range(img.height)]
    widest = max(range(img.height), key=lambda y: widths[y])
    bottom = next((y for y in range(widest, img.height) if widths[y] < TAIL_WIDTH * widths[widest]), img.height)
    alpha.paste(0, (0, bottom, img.width, img.height))
    box = alpha.getbbox()
    return img.crop(box) if box else img


def main():
    size = int(sys.argv[sys.argv.index("--size") + 1]) if "--size" in sys.argv else 96
    force = "--force" in sys.argv
    from generate import goods_icons
    icon_of = goods_icons()
    listed = notes()
    images = atlas_icons({icon_of.get(n) for n, _, _ in listed})
    os.makedirs(OUT_DIR, exist_ok=True)
    written = kept = 0
    for note, cls, which in listed:
        path = os.path.join(OUT_DIR, file_name(cls, which))
        img = images.get(icon_of.get(note))
        if img is None:
            print(f"  no icon found for {cls} ({which})")
        elif os.path.exists(path) and not force:
            kept += 1
        else:
            art = trim(img)
            scale = size * ART_SHARE / max(art.width, art.height)
            art = art.resize((round(art.width * scale), round(art.height * scale)), Image.LANCZOS)
            square = Image.new("RGBA", (size, size), (0, 0, 0, 0))
            square.alpha_composite(art, ((size - art.width) // 2, size - art.height))
            square.save(path)
            written += 1
    print(f"Note icons in {OUT_DIR}: wrote {written}, kept {kept} existing")


if __name__ == "__main__":
    main()
