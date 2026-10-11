"""Export Convergence's class note icons as a starting point for editing, in two sets the plugin
reads at every start and shows as they are (a note without a file keeps Convergence's icon in
descriptions, the stock icon on the map):

    icons/notes/<Class> (path|weapons).png  next to the class name in item descriptions
                                            (plugin/src/noteicons.rs), cut as described below
    icons/map/<Class> (path|weapons).png    the map markers (plugin/src/gfx.rs): the whole note at
                                            full size, tail included, framed so the paper is the
                                            image's centre, with an uneven black outline and a glow
                                            in the class colour, like the map's own icons
    icons/map/<Class> (<Tier> rune).png     the class's spell rune markers, one per tier
                                            ("Stormcaller (Shining rune).png"): that rune's detail
                                            image, with the same outline and glow

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
import colorsys
import io
import json
import os
import random
import re
import struct
import sys

from PIL import Image, ImageChops, ImageFilter

import common
from dump_fmg import bnd4, dcx

OUT_DIR = os.path.join(common.ROOT, "icons", "notes")
MAP_DIR = os.path.join(common.ROOT, "icons", "map")
MENU_HI = os.path.join(common.MOD, "menu", "hi")
TAIL_WIDTH = 0.15   # rows narrower than this share of the paper's width are the tail
ART_SHARE = 2 / 3   # the note's widest side, as a share of the square's side

# Map icons: room around the note for outline and glow (share of the note's square, each edge),
# the outline (thin / thick rim in px, how far apart its bumps are in px, colour) and the glow
# (blur radius as a share of the icon, strength). DEFAULT_GLOW is for a class without a colour.
MAP_PAD = 0.14
OUTLINE_THIN, OUTLINE_THICK, OUTLINE_WOBBLE = 1, 1, 30
OUTLINE_SOFT = 0.5   # blur of the outline edge, px
OUTLINE_COLOUR = (12, 10, 8)
GLOW_RADIUS, GLOW_GAIN = 0.035, 1.6
DEFAULT_GLOW = (224, 200, 154)
RUNE_PX = 168          # rune images are scaled to this (the note map icons' size) before outline and glow
RUNE_SOLID = 160      # rune art fades out in a dark haze: the outline follows only its solid part


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


def paper_box(img):
    """The box of the note's paper, without the tail hanging below it. Going down from the
    paper's widest row, the paper ends at the first row narrower than TAIL_WIDTH of it."""
    alpha = img.getchannel("A").point(lambda a: 255 if a > 16 else 0)
    widths = [sum(1 for x in range(img.width) if alpha.getpixel((x, y))) for y in range(img.height)]
    widest = max(range(img.height), key=lambda y: widths[y])
    bottom = next((y for y in range(widest, img.height) if widths[y] < TAIL_WIDTH * widths[widest]), img.height)
    alpha.paste(0, (0, bottom, img.width, img.height))
    return alpha.getbbox()


def trim(img):
    """The note without its tail and empty margins."""
    box = paper_box(img)
    return img.crop(box) if box else img


def text_icon(img, size):
    """For item descriptions: the trimmed note at ART_SHARE of a `size` square, on its bottom edge."""
    art = trim(img)
    scale = size * ART_SHARE / max(art.width, art.height)
    art = art.resize((round(art.width * scale), round(art.height * scale)), Image.LANCZOS)
    square = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    square.alpha_composite(art, ((size - art.width) // 2, size - art.height))
    return square


def class_colour(icon_path):
    """The text colour the plugin gives a class (plugin/src/colors.rs, same rule): the average of
    the most vivid fifth of the icon's pixels, saturation capped, brightened, and lightened towards
    white if still dark. None if the icon can't be read."""
    try:
        im = Image.open(icon_path).convert("RGBA")
    except OSError:
        return None
    scored = []
    for r, g, b, a in im.get_flattened_data():
        if a > 128:
            h, s, v = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
            scored.append((s * v, (r / 255, g / 255, b / 255)))
    if not scored:
        return None
    scored.sort(key=lambda t: -t[0])
    top = scored[:max(1, len(scored) // 5)]
    avg = [sum(c[i] for _, c in top) / len(top) for i in range(3)]
    h, s, v = colorsys.rgb_to_hsv(*avg)
    c = list(colorsys.hsv_to_rgb(h, min(s, 0.65), max(v, 0.88)))
    lum = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    if lum < 0.5:
        t = (0.5 - lum) / (1 - lum)
        c = [x + (1 - x) * t for x in c]
    return tuple(round(x * 255) for x in c)


def colour_overrides():
    """{class: (r, g, b)} from icons/colors.txt (Class = #RRGGBB), as the plugin reads it."""
    out = {}
    try:
        lines = open(os.path.join(common.ROOT, "icons", "colors.txt"), encoding="utf-8").read().splitlines()
    except OSError:
        return out
    for line in lines:
        m = re.fullmatch(r"\s*([^#=][^=]*?)\s*=\s*#([0-9A-Fa-f]{6})\s*", line)
        if m:
            v = int(m.group(2), 16)
            out[m.group(1).lower()] = (v >> 16, (v >> 8) & 255, v & 255)
    return out


def map_icon(img, glow, seed, on_paper=True, solid=40):
    """For map markers, like the map's own icons: the whole image at full size, framed so the
    centre of a note's paper (`on_paper`; tail included in the image) or of the whole art is the
    image's centre; around it an uneven black outline (its thickness wanders, as if drawn by hand)
    and a soft glow in `glow` colour. The outline follows the pixels more opaque than `solid` (0-255).
    `seed` makes the outline's wobble the same on every export."""
    art = img.getchannel("A").point(lambda a: 255 if a > 16 else 0).getbbox() or (0, 0, img.width, img.height)
    paper = (paper_box(img) if on_paper else None) or art
    cx, cy = (paper[0] + paper[2]) / 2, (paper[1] + paper[3]) / 2
    reach = max(cx - art[0], art[2] - cx, cy - art[1], art[3] - cy)
    side = 2 * int(reach + 0.999) + 2
    pad = round(side * MAP_PAD)
    full = side + 2 * pad
    note = Image.new("RGBA", (full, full), (0, 0, 0, 0))
    note.alpha_composite(img, (round(full / 2 - cx), round(full / 2 - cy)))

    # Outline: a thin rim everywhere, thickened where a smooth random field is high.
    shape = note.getchannel("A").point(lambda a: 255 if a > solid else 0)
    thin = shape.filter(ImageFilter.MaxFilter(2 * OUTLINE_THIN + 1))
    thick = shape.filter(ImageFilter.MaxFilter(2 * OUTLINE_THICK + 1))
    rng = random.Random(seed)
    cells = max(2, full // OUTLINE_WOBBLE)
    field = Image.new("L", (cells, cells))
    field.putdata([rng.randrange(256) for _ in range(cells * cells)])
    field = field.resize((full, full), Image.BICUBIC).filter(ImageFilter.GaussianBlur(OUTLINE_WOBBLE / 4))
    wobble = ImageChops.multiply(thick, field.point(lambda v: 255 if v > 120 else 0))
    rim = ImageChops.lighter(thin, wobble).filter(ImageFilter.GaussianBlur(OUTLINE_SOFT))
    rim = rim.point(lambda v: min(255, round(v * 1.3)))
    outline = Image.new("RGBA", (full, full), OUTLINE_COLOUR + (0,))
    outline.putalpha(rim)

    # Glow outside the outline.
    halo = rim.filter(ImageFilter.GaussianBlur(full * GLOW_RADIUS)).point(lambda v: min(255, round(v * GLOW_GAIN)))
    glow_layer = Image.new("RGBA", (full, full), glow + (0,))
    glow_layer.putalpha(halo)

    out = Image.new("RGBA", (full, full), (0, 0, 0, 0))
    for layer in (glow_layer, outline, note):
        out.alpha_composite(layer)
    return out


def detail_images(wanted):
    """{icon id: RGBA image} from the items' large detail images (menu/hi/00_solo.tpfbhd/bdt,
    menu_knowledge_<id>), which exist for every rune tier; many small atlas icons are blank."""
    bhd = open(os.path.join(MENU_HI, "00_solo.tpfbhd"), "rb").read()
    count = struct.unpack_from("<i", bhd, 0x0C)[0]
    hsize = struct.unpack_from("<q", bhd, 0x20)[0]
    fmt = int(f"{bhd[0x31]:08b}"[::-1], 2)
    index = {}
    for i in range(count):
        o = 0x40 + i * hsize
        csize, = struct.unpack_from("<q", bhd, o + 8)
        q = o + 16 + (8 if fmt & 0x20 else 0)
        off, = struct.unpack_from("<I", bhd, q)
        q += 4 + (4 if fmt & 0x02 else 0)
        no, = struct.unpack_from("<I", bhd, q)
        index[bhd[no:bhd.index(b"\0", no)].decode("shift_jis").lower().split("\\")[-1]] = (off, csize)
    out = {}
    with open(os.path.join(MENU_HI, "00_solo.tpfbdt"), "rb") as bdt:
        for icon in wanted:
            hit = index.get(f"menu_knowledge_{icon:05}.tpf.dcx")
            if hit:
                bdt.seek(hit[0])
                raw = bdt.read(hit[1])
                tpf = dcx(raw) if raw[:4] == b"DCX\0" else raw
                off, size = struct.unpack_from("<Ii", tpf, 0x10)
                out[icon] = Image.open(io.BytesIO(tpf[off:off + size])).convert("RGBA")
    return out


def rune_icons(icon_of):
    """{(class, tier): rune icon id}: for each class whose notes list spell runes, every tier of
    its school ("Shimmering" from "Shimmering Rune of the Storm")."""
    import generate
    names = {}
    for f in ("GoodsName", "GoodsName_dlc01", "GoodsName_dlc02"):
        names.update({int(k): v for k, v in json.load(open(os.path.join(common.ITEM_DIR, f + ".fmg.json"),
                                                            encoding="utf-8")).items() if v})
    by_school = {}
    for g in generate.rune_spells():
        if g in names and icon_of.get(g):
            tier, school = names[g].split(" ", 1)
            by_school.setdefault(school, {})[tier] = icon_of[g]
    out = {}
    for c in json.load(open(os.path.join(common.CACHE, "classes.json"), encoding="utf-8")):
        for rune, _ in c["runes"]:
            for tier, icon in by_school.get(rune.split(" ", 1)[-1], {}).items():
                out[(c["name"], tier)] = icon
    return out


def main():
    size = int(sys.argv[sys.argv.index("--size") + 1]) if "--size" in sys.argv else 96
    force = "--force" in sys.argv
    from generate import goods_icons
    icon_of = goods_icons()
    listed = notes()
    runes = rune_icons(icon_of)
    images = atlas_icons({icon_of.get(n) for n, _, _ in listed})
    rune_images = detail_images(set(runes.values()))
    overrides = colour_overrides()

    def glow_of(cls):
        # The class's text colour: colors.txt, else from its description icon (path first).
        if cls.lower() in overrides:
            return overrides[cls.lower()]
        for which in ("path", "weapons"):
            c = class_colour(os.path.join(OUT_DIR, file_name(cls, which)))
            if c:
                return c
        return DEFAULT_GLOW

    # The description icons first: the map icons' glow takes its colour from them.
    sets = ((OUT_DIR, lambda img, cls, which: text_icon(img, size)),
            (MAP_DIR, lambda img, cls, which: map_icon(img, glow_of(cls), f"{cls} ({which})")))
    for out_dir, make in sets:
        os.makedirs(out_dir, exist_ok=True)
        written = kept = 0
        for note, cls, which in listed:
            path = os.path.join(out_dir, file_name(cls, which))
            img = images.get(icon_of.get(note))
            if img is None:
                print(f"  no icon found for {cls} ({which})")
            elif os.path.exists(path) and not force:
                kept += 1
            else:
                make(img, cls, which).save(path)
                written += 1
        print(f"Note icons in {out_dir}: wrote {written}, kept {kept} existing")

    # Rune markers: each tier of the class's school rune (its detail image), framed on the whole art.
    written = kept = 0
    for (cls, tier), icon in runes.items():
        which = f"{tier} rune"
        path = os.path.join(MAP_DIR, file_name(cls, which))
        if os.path.exists(path) and not force:
            kept += 1
        elif icon in rune_images:
            art = rune_images[icon]
            art = art.crop(art.getchannel("A").getbbox() or (0, 0, art.width, art.height))
            scale = RUNE_PX / max(art.width, art.height)
            art = art.resize((round(art.width * scale), round(art.height * scale)), Image.LANCZOS)
            map_icon(art, glow_of(cls), f"{cls} ({which})", on_paper=False, solid=RUNE_SOLID).save(path)
            written += 1
    print(f"Rune icons in {MAP_DIR}: wrote {written}, kept {kept} existing")


if __name__ == "__main__":
    main()
