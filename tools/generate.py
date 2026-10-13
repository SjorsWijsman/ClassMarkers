"""Generate the Class Markers plugin's marker data from The Convergence's own files.

    python generate.py

1. Extracts regulation.bin params and the English item and menu text from ConvergenceER\\mod into
   tools\\cache (skipped when the sources haven't changed since the last run).
2. Parses the class notes into per-class item lists.
3. Locates every listed item (pickups, boss/NPC rewards) in the map and event files.
4. Writes plugin\\src\\generated.rs, then the plugin is rebuilt with `cargo build --release`.
"""
import json
import re
import os
import struct
from collections import Counter, defaultdict

import common

# Map areas whose items are placed on the dungeon's entrance icon instead of their exact spot:
# caves, catacombs, tunnels, gaols and the like, which aren't drawn on the world map.
ENTRANCE_AREAS = {30, 31, 32, 40, 41, 42, 43}
OVERWORLD_AREAS = {60, 61}
BOSS_ICON = 499           # Convergence's boss marker icon; never used as an entrance
ITEM_KIND = {"rune": 1, "weapon": 2, "armor": 3}   # ItemLot category numbers


def extract():
    """Decode Convergence's regulation.bin and English item and menu text into the cache."""
    from dump_fmg import extract_msgbnd
    from reg import extract_regulation

    sources = [
        os.path.join(common.MOD, "regulation.bin"),
        os.path.join(common.MOD, "msg", "engus", "item_dlc02.msgbnd.dcx"),
        os.path.join(common.MOD, "msg", "engus", "menu_dlc02.msgbnd.dcx"),
    ]
    stamp_path = os.path.join(common.CACHE, "sources.json")
    stamp = {s: [os.path.getsize(s), int(os.path.getmtime(s))] for s in sources}
    if (os.path.exists(stamp_path) and json.load(open(stamp_path)) == {k: v for k, v in stamp.items()}
            and os.path.exists(os.path.join(common.ITEM_DIR, "file_ids.json"))):
        print("Convergence files unchanged since the last run - using the cache.")
        return
    os.makedirs(common.CACHE, exist_ok=True)
    print("Extracting regulation.bin ...")
    extract_regulation(sources[0], common.REG_DIR)
    print("Extracting item and menu text ...")
    extract_msgbnd(sources[1], common.ITEM_DIR)
    extract_msgbnd(sources[2], common.MENU_DIR)
    json.dump(stamp, open(stamp_path, "w"))


def world_map_rows():
    from param import rows
    r, _ = rows(os.path.join(common.REG_DIR, "WorldMapPointParam.param"))
    return r


def row_info(rid, b):
    return {
        "id": rid,
        "area": b[0x20], "gx": b[0x21], "gz": b[0x22],
        "pos": struct.unpack_from("<3f", b, 0x24),
        "icon": struct.unpack_from("<H", b, 0x0C)[0],
        "disp_mask": b[0x18],
        "text_id": struct.unpack_from("<i", b, 0x30)[0],
    }


def small_tile(mapname, pos):
    """Overworld maps also exist as low-detail copies covering 2x2 (m60_XX_ZZ_01) or 4x4 (_02)
    tiles, with positions relative to that bigger tile (the digit before the level is a map
    variant, e.g. _12). -> the regular tile and its position."""
    level = int(mapname[11])
    if mapname[1:3] not in ("60", "61") or level == 0:
        return mapname, pos
    n = 2 ** level
    out = []
    for big, local in ((int(mapname[4:6]), pos[0]), (int(mapname[7:9]), pos[2])):
        world = big * n * 256 + n * 128 + local
        tile = int(world // 256)
        out.append((tile, world - tile * 256 - 128))
    (bx, x), (bz, z) = out
    return f"m{mapname[1:3]}_{bx:02d}_{bz:02d}_00", (x, pos[1], z)


class Placer:
    """Turns an item location (map + position) into a world-map row placement.

    A placement is (block, area, gx, gz, x, y, z, disp_mask, text_id). The row id the plugin
    gives it is block * 100 + a free two-digit number, because the map only draws rows whose
    id encodes their map block: 6 digits AA BB NN for dungeon maps (mAA_BB), 8 digits
    PP GG ZZ NN for world-map tiles. Verified in-game with test build 0.1.6.
    """

    def __init__(self, wmp, place_names=None):
        """`wmp`: WorldMapPointParam rows (id -> bytes); `place_names`: PlaceName text id -> text
        (default: Convergence's, from the cache)."""
        self.rows = {rid: row_info(rid, b) for rid, b in wmp.items()}
        self.by_block = defaultdict(list)
        for rid, info in self.rows.items():
            self.by_block[rid // 100].append(info)
        # Tile prefix (PP) per overworld tile, from Convergence's own 8-digit rows.
        self.tile_prefix = defaultdict(Counter)
        for rid, info in self.rows.items():
            s = str(rid)
            if len(s) == 8 and info["area"] in OVERWORLD_AREAS and s[2:4] == f"{info['gx']:02d}" \
                    and s[4:6] == f"{info['gz']:02d}" and s[0] in "68":
                self.tile_prefix[(info["area"], info["gx"], info["gz"])][int(s[:2])] += 1
        if place_names is None:
            place_names = {}
            for f in ["PlaceName", "PlaceName_dlc01", "PlaceName_dlc02"]:
                p = os.path.join(common.ITEM_DIR, f + ".fmg.json")
                place_names.update({int(k): v for k, v in json.load(open(p, encoding="utf-8")).items()})
        self.place_names = place_names

    def prefix_for_tile(self, area, gx, gz):
        here = self.tile_prefix.get((area, gx, gz))
        if here:
            return here.most_common(1)[0][0]
        # Nearest tile that has one (same overworld).
        best = min(((abs(k[1] - gx) + abs(k[2] - gz), c.most_common(1)[0][0])
                    for k, c in self.tile_prefix.items() if k[0] == area), default=None)
        return best[1] if best else (61 if area == 60 else 68)

    def nearest_place_name(self, area, gx, gz, x, z):
        """Text id of the closest named location row in the same area (label fallback).
        Overworld rows are compared in world coordinates, dungeon rows within the same map."""
        overworld = area in OVERWORLD_AREAS
        wx, wz = (gx * 256 + x, gz * 256 + z) if overworld else (x, z)
        best = None
        for info in self.rows.values():
            if info["area"] != area or info["icon"] == BOSS_ICON or info["text_id"] not in self.place_names:
                continue
            if not overworld and info["gx"] != gx:
                continue
            ox, oz = info["pos"][0], info["pos"][2]
            if overworld:
                ox, oz = info["gx"] * 256 + ox, info["gz"] * 256 + oz
            d = (ox - wx) ** 2 + (oz - wz) ** 2
            if best is None or d < best[0]:
                best = (d, info["text_id"])
        return best[1] if best else -1

    def entrance_row(self, aa, bb):
        """The dungeon's own map icon (id AABB00..), not a boss marker."""
        cands = sorted((i for i in self.by_block.get(aa * 100 + bb, []) if i["icon"] != BOSS_ICON),
                       key=lambda i: i["id"])
        return cands[0] if cands else None

    def disp_mask_for_area(self, aa, bb):
        masks = Counter(i["disp_mask"] for i in self.by_block.get(aa * 100 + bb, []))
        if masks:
            return masks.most_common(1)[0][0]
        return 2 if aa == 12 else 4 if (20 <= aa <= 28 or 40 <= aa <= 43 or aa in (17, 61)) else 1

    def place(self, mapname, pos):
        mapname, pos = small_tile(mapname, pos)
        aa, bb, cc = int(mapname[1:3]), int(mapname[4:6]), int(mapname[7:9])
        if aa in OVERWORLD_AREAS:
            pp = self.prefix_for_tile(aa, bb, cc)
            return dict(block=pp * 10000 + bb * 100 + cc, area=aa, gx=bb, gz=cc,
                        x=pos[0], y=pos[1], z=pos[2], disp_mask=1 if aa == 60 else 4,
                        text_id=self.nearest_place_name(aa, bb, cc, pos[0], pos[2]), how="exact")
        if aa in ENTRANCE_AREAS:
            e = self.entrance_row(aa, bb)
            if e:
                return dict(block=e["id"] // 100, area=e["area"], gx=e["gx"], gz=e["gz"],
                            x=e["pos"][0], y=e["pos"][1], z=e["pos"][2], disp_mask=e["disp_mask"],
                            text_id=e["text_id"], how="entrance")
        e = self.entrance_row(aa, bb)
        text_id = e["text_id"] if e else self.nearest_place_name(aa, bb, 0, pos[0], pos[2])
        return dict(block=aa * 100 + bb, area=aa, gx=bb, gz=0, x=pos[0], y=pos[1], z=pos[2],
                    disp_mask=self.disp_mask_for_area(aa, bb), text_id=text_id, how="exact-dungeon")

    def place_on_map(self, mapname, prefer_boss, loc):
        """A placement when only the map is known: a boss marker there that the note's location
        text `loc` names a word of ("Dakk" -> "Dakk, Starcaller Lord"; first when `prefer_boss`:
        the note talks about a defeat, drop or invasion), else the dungeon's entrance or a named
        place in the overworld tile, else the tile's middle. None if there's nothing to put it on.
        A boss the note doesn't name is never used: its label would point at the wrong fight."""
        mapname, _ = small_tile(mapname, (0.0, 0.0, 0.0))
        aa, bb, cc = int(mapname[1:3]), int(mapname[4:6]), int(mapname[7:9])
        if aa in OVERWORLD_AREAS:
            here = [i for i in self.rows.values() if (i["area"], i["gx"], i["gz"]) == (aa, bb, cc)]
        else:
            here = list(self.by_block.get(aa * 100 + bb, []))
        from matching import norm
        def words(text):   # words, and neighbours joined ("Death Bird" also says "deathbird")
            w = norm(text)
            return {x for x in w + [p + q for p, q in zip(w, w[1:])] if len(x) >= 4}
        said = words(loc)
        bosses = [i for i in here if i["icon"] == BOSS_ICON and said & words(self.place_names.get(i["text_id"], ""))]
        named = [i for i in here if i["icon"] != BOSS_ICON and i["text_id"] in self.place_names]
        entrance = None if aa in OVERWORLD_AREAS else self.entrance_row(aa, bb)
        for pick in ((bosses if prefer_boss else []) + ([entrance] if entrance else []) + named + bosses):
            p = self.place_on_row(pick["id"])
            p["how"] = "map-area"
            return p
        if aa in OVERWORLD_AREAS:
            return dict(block=self.prefix_for_tile(aa, bb, cc) * 10000 + bb * 100 + cc, area=aa, gx=bb, gz=cc,
                        x=0.0, y=0.0, z=0.0, disp_mask=1 if aa == 60 else 4,
                        text_id=self.nearest_place_name(aa, bb, cc, 0.0, 0.0), how="map-area")
        return None

    def place_on_row(self, rid):
        r = self.rows[rid]
        return dict(block=rid // 100, area=r["area"], gx=r["gx"], gz=r["gz"], x=r["pos"][0],
                    y=r["pos"][1], z=r["pos"][2], disp_mask=r["disp_mask"], text_id=r["text_id"],
                    how="boss-marker")


SAME_SPOT_M = 10


def build_markers(classes, report, placer):
    import locate
    out, unplaced = [], []
    # A marker belongs to the note that lists it: runes and armor are in "Path of the <class>",
    # weapons in "Weapons for <class>".
    path_note = {c["name"]: c["pathId"] for c in classes}
    weapons_note = {c["name"]: c.get("weaponsId") for c in classes}
    for entry in report:
        placed_any = False
        for res in entry["results"]:
            if res["how"] in locate.PLACED:
                p = placer.place(res["map"], res["pos"])
            elif res["how"] in ("boss-marker", "place-name", "npc-marker"):
                p = placer.place_on_row(res["row"])
                p["how"] = res["how"]
            elif res["how"] == "map-area":
                p = placer.place_on_map(res["map"], res["boss"], entry["loc"])
                if p is None:
                    continue
            else:
                continue
            note = weapons_note[entry["class"]] if entry["kind"] == "weapon" else path_note[entry["class"]]
            items = [(r["cat"], r["item_id"]) for r in entry["results"] if "item_id" in r]
            out.append(dict(p, cls=entry["class"], note=note, entry=entry["name"], loc=entry["loc"],
                            name_item=name_item(entry["name"], items),
                            line=line_of(note, entry["name"], entry["loc"], set()),
                            kind=ITEM_KIND[entry["kind"]], item_id=res["item_id"], item=res["piece"], cat=res["cat"],
                            flag=res.get("flag", 0) or 0, src=f'{res["how"]} {res.get("map", "")}'.strip()))
            placed_any = True
        if not placed_any:
            unplaced.append(entry)
    # Armor sets: when the pieces come from different places and one place has clearly the most
    # pieces, that's the set the note means; a lone piece elsewhere (Pelt of Ralva next to the
    # Iron Rivet set) gets no marker. Places with equally many pieces all stay.
    spot = lambda m: (m["block"], round(m["x"]), round(m["z"]))
    by_entry = {}
    for m in out:
        by_entry.setdefault((m["cls"], m["entry"]), []).append(m)
    kept = []
    for (cls, entry), ms in by_entry.items():
        if entry.endswith(" Set") or entry.startswith("Set of "):
            pieces_at = Counter()
            for s in {(spot(m), m["item"]) for m in ms}:
                pieces_at[s[0]] += 1
            top = max(pieces_at.values())
            if top >= 2:
                ms = [m for m in ms if pieces_at[spot(m)] == top]
        kept += ms
    out = kept

    # One marker per note entry per spot: an armor set's pieces usually lie together. The marker
    # keeps every item found there; it hides once you've obtained all of them.
    # Within 10 m counts as the same spot (one NPC placed twice for different quest states).
    unique = []
    for m in out:
        first = next((u for u in unique if (u["cls"], u["entry"], u["block"]) == (m["cls"], m["entry"], m["block"])
                      and (u["x"] - m["x"]) ** 2 + (u["z"] - m["z"]) ** 2 <= SAME_SPOT_M ** 2), None)
        if first is None:
            first = dict(m, items=[])
            unique.append(first)
        item = (m["cat"], m["item_id"], m["flag"])
        if item not in first["items"]:
            first["items"].append(item)
    return unique, unplaced


def note_flags(classes):
    """[(note goods id, class, which note, [flags set when it's picked up])] for every class note.
    The flags are the getItemFlags of every item lot that hands out the note."""
    import locate
    out = []
    for c in classes:
        for which, note in (("path", c["pathId"]), ("weapons", c.get("weaponsId"))):
            if note:
                flags = sorted({flag for _, _, flag in locate.LOTS.get((1, note), []) if flag > 0})
                out.append((note, c["name"], which, flags))
    return out


def rust_str(s):
    return json.dumps(s, ensure_ascii=False)


def goods_icons():
    """Goods id -> its icon id (EquipParamGoods iconId)."""
    from param import rows
    r, _ = rows(os.path.join(common.REG_DIR, "EquipParamGoods.param"))
    return {gid: struct.unpack_from("<H", b, 0x30)[0] for gid, b in r.items()}


# Character creation: the class list's rows (CharMakeMenuListItemParam) are CLASS_LIST + n, and the
# class starting from list row CLASS_LIST + n gets CharaInitParam row CLASS_START + n.
CLASS_LIST = 100200
CLASS_START = 3000
# CharaInitParam fields with starting gear: (ItemLot category, first offset, count). Arrows, bolts
# and the flasks and other goods every class starts with are left out.
START_GEAR = [(2, 0x10, 4), (3, 0x20, 4), (4, 0x40, 4), (1, 0x60, 7)]


def class_start_events():
    """{class number: [goods ids]} that an event gives a class at the start. Convergence does that
    with $Event(N, ...) { EndIf(... !PlayersClass(X4_4)); ... DirectlyGivePlayerItem(ItemType.Goods,
    X8_4, ...) ... }, started as InitializeEvent(slot, N, flag, class number, goods, ...). The class
    number is the one CharaInitParam rows count with (CLASS_START + n)."""
    import glob
    events = {}   # event id -> argument indexes that are goods given
    texts = [open(f, encoding="utf-8").read() for f in glob.glob(os.path.join(common.MOD, "event", "*.emevd.dcx.js"))]
    for text in texts:
        for m in re.finditer(r"\$Event\((\d+),[^{]*\{(.*?)\n\}\);", text, re.S):
            body = m.group(2)
            if "PlayersClass(X4_4)" in body:
                given = [int(x) // 4 for x in re.findall(r"DirectlyGivePlayerItem\(ItemType\.Goods, X(\d+)_4", body)]
                if given:
                    events[int(m.group(1))] = given
    out = defaultdict(list)
    for text in texts:
        for event, args in re.findall(r"InitializeEvent\(\d+, (\d+), ([-\d, ]+)\)", text):
            if int(event) in events:
                args = [int(a) for a in args.split(",")]
                out[args[1]] += [args[i] for i in events[int(event)] if i < len(args) and args[i] > 0]
    return out


def starting_gear():
    """{class name: [(category, item id)]} for every class you can pick at character creation:
    its CharaInitParam gear plus the goods an event gives it at the start."""
    from param import rows
    menu = json.load(open(os.path.join(common.MENU_DIR, "GR_MenuText.fmg.json"), encoding="utf-8"))
    listed, _ = rows(os.path.join(common.REG_DIR, "CharMakeMenuListItemParam.param"))
    init, _ = rows(os.path.join(common.REG_DIR, "CharaInitParam.param"))
    given = class_start_events()
    out = {}
    for rid, b in listed.items():
        n = rid - CLASS_LIST
        start = CLASS_START + n
        if not 0 <= n < 100 or start not in init:
            continue
        name = menu.get(str(struct.unpack_from("<i", b, 4)[0]))
        gear = []
        for cat, first, count in START_GEAR:
            for k in range(count):
                item = struct.unpack_from("<i", init[start], first + 4 * k)[0]
                if item > 0:
                    gear.append((cat, item - item % 100 if cat == 2 else item))   # weapon: drop the upgrade level
        gear += [(1, g) for g in given.get(n, [])]
        if name:
            out[name] = gear
    return out


def rune_spells():
    """{goods id: [(category, item id)]}: what using an item teaches. Convergence's spell runes
    work like this: the goods' use effect (EquipParamGoods refId_default) is watched by an event,
    $Event(N, ...) { WaitFor(CharacterHasSpEffect(10000, X0_4)); AwardItems...(X4_4); ... },
    started as InitializeEvent(slot, N, effect, lot). Awarding a lot gives that ItemLotParam_map row
    and the rows right after it, up to the first missing id."""
    import glob
    from param import rows
    watchers = set()
    calls = []
    for f in glob.glob(os.path.join(common.MOD, "event", "*.emevd.dcx.js")):
        text = open(f, encoding="utf-8").read()
        for m in re.finditer(r"\$Event\((\d+),[^{]*\{(.*?)\n\}\);", text, re.S):
            if re.search(r"WaitFor\(CharacterHasSpEffect\(10000, X0_4\)\);\s*AwardItems\w*\(X4_4\)", m.group(2)):
                watchers.add(int(m.group(1)))
        calls += re.findall(r"InitializeEvent\(\d+, (\d+), (\d+), (\d+)\)", text)
    lot_of = {int(effect): int(lot) for event, effect, lot in calls if int(event) in watchers}
    goods, _ = rows(os.path.join(common.REG_DIR, "EquipParamGoods.param"))
    lots, _ = rows(os.path.join(common.REG_DIR, "ItemLotParam_map.param"))
    out = {}
    for gid, b in goods.items():
        lot = lot_of.get(struct.unpack_from("<i", b, 0x04)[0])
        items = []
        while lot in lots:
            ids = struct.unpack_from("<8i", lots[lot], 0)
            cats = struct.unpack_from("<8i", lots[lot], 0x20)
            items += [(c, i) for i, c in zip(ids, cats) if i > 0]
            lot += 1
        if items:
            out[gid] = items
    return out


def armor_set(pieces, groups=True):
    """Armor ids that belong with `pieces`. The game numbers a set's pieces in 1000-id groups
    (+0 head, +100 chest, +200 hands, +300 legs), so with `groups`: every piece in their groups
    (alternatives like the White Mask next to the War Surgeon Gown), plus the head-only groups
    right after one (the alternative crowns after the Raya Lucarian set). Always: the "(Altered)"
    version of every piece, which often sits in a group of its own."""
    armor = {i: n for (cat, i), n in item_names().items() if cat == 3 and n and "[ERROR]" not in n}
    group = lambda i: i - i % 1000
    out = set(pieces)
    if groups:
        blocks = {group(i) for i in pieces}
        members = defaultdict(list)
        for i in armor:
            members[group(i)].append(i)
        for b in list(blocks):
            nxt = b + 1000
            while members.get(nxt) and all(i == nxt for i in members[nxt]):
                blocks.add(nxt)
                nxt += 1000
        out |= {i for i in armor if group(i) in blocks}
    by_name = {n: i for i, n in armor.items()}
    out |= {by_name[armor[i] + " (Altered)"] for i in list(out) if armor.get(i, "") + " (Altered)" in by_name}
    return out - set(pieces)


def class_captions(classes):
    """[((category, item id), [goods ids of the notes listing it])] for every item a class note
    lists, a listed rune teaches, or a class starts with, one note per class, in class order (a
    starting item names the class's weapons note for a weapon, else its path note). A plain weapon
    also covers its infused variants (base + affinity * 100), which have their own descriptions;
    armor covers the rest of its set (armor_set); talismans and goods are exact ids."""
    import locate
    start = starting_gear()
    teaches = rune_spells()
    # Rune families: "Faint/Shimmering/... Rune of Night" are one school. The notes list the
    # school's later runes; a class gets every rune of the schools its notes list.
    goods_names = {}
    for f in ("GoodsName", "GoodsName_dlc01", "GoodsName_dlc02"):
        goods_names.update({int(k): v for k, v in
                            json.load(open(os.path.join(common.ITEM_DIR, f + ".fmg.json"), encoding="utf-8")).items() if v})
    family = lambda gid: goods_names.get(gid, "").split(" ", 1)[-1]
    by_family = defaultdict(set)
    for gid in teaches:
        by_family[family(gid)].add(gid)
    missing = [c["name"] for c in classes if c["name"] not in start]
    if missing:
        print("No starting class found for:", ", ".join(missing))
    weapon_variants = defaultdict(list)          # plain weapon id -> variant ids with a caption
    for f in ("WeaponCaption", "WeaponCaption_dlc01", "WeaponCaption_dlc02"):
        for k in json.load(open(os.path.join(common.ITEM_DIR, f + ".fmg.json"), encoding="utf-8")):
            i = int(k)
            if i % 100 == 0 and i % 10000:
                weapon_variants[i - i % 10000].append(i)
    out = {}
    for c in classes:
        items = []                                # (category, item id, note)
        for kind, name, loc, siblings in locate.note_entries(c):
            note = (kind == "weapon" and c.get("weaponsId")) or c["pathId"]
            for _, item, cat in locate.resolve_ids(kind, name, siblings):
                items.append((cat, item, note))
                if cat == 1 and item in teaches:
                    for rune in by_family[family(item)]:
                        items.append((1, rune, note))
                        items += [(c, i, note) for c, i in teaches[rune]]
        # A listed armor set covers its whole set (armor_set). A piece the notes list on its own
        # only covers its altered version, unless the notes list another piece of its group too.
        listed = Counter(i - i % 1000 for cat, i, _ in items if cat == 3)
        whole = {i for cat, i, _ in items if cat == 3 and listed[i - i % 1000] > 1}
        alone = {i for cat, i, _ in items if cat == 3} - whole
        items += [(3, i, c["pathId"]) for i in armor_set(whole) | armor_set(alone, groups=False)]
        gear = start.get(c["name"], [])
        gear = gear + [(3, i) for i in armor_set({i for cat, i in gear if cat == 3})]
        for cat, item in gear:
            items.append((cat, item, (cat == 2 and c.get("weaponsId")) or c["pathId"]))
        for cat, item, note in items:
            if cat not in (1, 2, 3, 4):
                continue
            ids = [item] + (weapon_variants.get(item, []) if cat == 2 and item % 10000 == 0 else [])
            for i in ids:
                listed = out.setdefault((cat, i), [])
                if all(cls != c["name"] for cls, _ in listed):
                    listed.append((c["name"], note))
    return sorted(out.items())


_ITEM_NAMES = {}


def item_names():
    """{(ItemLot category, id): name} from the game's item name text (later layers win)."""
    if not _ITEM_NAMES:
        for cat, base in ((1, "GoodsName"), (2, "WeaponName"), (3, "ProtectorName"), (4, "AccessoryName")):
            for suffix in ("", "_dlc01", "_dlc02"):
                path = os.path.join(common.ITEM_DIR, base + suffix + ".fmg.json")
                if os.path.exists(path):
                    _ITEM_NAMES.update({(cat, int(k)): v for k, v in json.load(open(path, encoding="utf-8")).items() if v})
    return _ITEM_NAMES


_CAPTIONS = {}


def goods_captions():
    """{goods id: description} (later layers win)."""
    if not _CAPTIONS:
        for f in ("GoodsCaption", "GoodsCaption_dlc01", "GoodsCaption_dlc02"):
            _CAPTIONS.update({int(k): v for k, v in json.load(open(os.path.join(common.ITEM_DIR, f + ".fmg.json"),
                                                                    encoding="utf-8")).items() if v})
    return _CAPTIONS


def note_lines(note):
    """A class note's entry lines in order, split exactly as the plugin splits the note's text at
    startup (plugin/src/notetext.rs): font tags removed, lines holding " - " are (name, location)."""
    text = re.sub(r"</?font[^>]*>", "", goods_captions().get(note, ""))
    out = []
    for line in text.split("\n"):
        line = line.strip()
        if " - " in line:
            name, loc = line.split(" - ", 1)
            out.append((name.strip(), loc.strip()))
    return out


def line_of(note, name, loc, taken):
    """The index of note entry (name, loc) among note_lines(note); `taken` avoids reusing one."""
    for i, nl in enumerate(note_lines(note)):
        if nl == (name.strip(), loc.strip()) and (note, i) not in taken:
            taken.add((note, i))
            return i
    raise ValueError(f"note {note} has no line {name!r} - {loc!r}")


def name_item(name, items):
    """The one item an entry is, if it's exactly one item and not a set: the plugin then names it as
    the game does (the notes have typos: "Blaphemous Blade"). As a Rust Option."""
    ids = {(cat, item) for cat, item, *_ in items}
    if len(ids) == 1 and not (name.endswith(" Set") or name.startswith("Set of ")):
        return "Some((%d, %d))" % next(iter(ids))
    return "None"


def intro_note(c):
    """The note whose "primarily uses" line both of a class's notes show (path note first)."""
    for note in (c["pathId"], c.get("weaponsId")):
        if note and re.search(r"primarily uses|favors", goods_captions().get(note, "")):
            return note
    return c["pathId"]


def note_texts(classes, report):
    """The class notes as the plugin rebuilds them (plugin/src/tracker.rs), as numbers only: per
    note (goods id, intro note, [(kind, source note, [(line, name item, items, starts a group)])]).
    The plugin reads names, locations and the intro from Convergence's own text by line number, so
    none of Convergence's words are copied here. Kind: 1 runes, 3 armor, 2 weapons."""
    items_of = defaultdict(set)
    for e in report:
        for res in e["results"]:
            if "item_id" in res:
                items_of[(e["class"], e["name"])].add((res["cat"], res["item_id"], res.get("flag", 0) or 0))
    out = []
    for c in classes:
        cls, taken = c["name"], set()
        path, weapons_note = c["pathId"], c.get("weaponsId")

        def entry(note, name, loc, first):
            items = sorted(items_of[(cls, name)])
            return (line_of(note, name, loc, taken), name_item(name, items), items, first)
        sections = []
        if c["runes"]:
            sections.append((1, path, [entry(path, n, l, False) for n, l in c["runes"]]))
        if c["armor"]:
            sections.append((3, path, [entry(path, n, l, k > 0 and i == 0)
                                       for k, group in enumerate(c["armor"]) for i, (n, l) in enumerate(group)]))
        weapons = [(2, weapons_note, [entry(weapons_note, n, l, False) for n, l in c["weapons"]])] if c["weapons"] else []
        if weapons_note:
            out.append((path, intro_note(c), sections))
            out.append((weapons_note, intro_note(c), weapons))
        else:
            out.append((path, intro_note(c), sections + weapons))
    return out


def rune_art(classes):
    """[(class, tier, icon id)] for every rune of the schools each class's notes list: the plugin
    makes each tier's map icon from that icon's detail image (plugin/src/iconcache.rs)."""
    icons = goods_icons()
    out = []
    for c in classes:
        schools = {r.split(" ", 1)[-1] for r, _ in c["runes"]}
        for gid in sorted(rune_spells()):
            tier, _, school = item_names().get((1, gid), "").partition(" ")
            if school in schools and icons.get(gid):
                out.append((c["name"], tier, icons[gid]))
    return out


def text_slots():
    """Rust constants for the item text files we read and change: their file ids in item.msgbnd,
    which are the game's MsgRepository slots."""
    ids = json.load(open(os.path.join(common.ITEM_DIR, "file_ids.json"), encoding="utf-8"))

    def slot(name):
        if name not in ids:
            raise SystemExit(f"{name}.fmg is missing from item.msgbnd")
        return ids[name]

    def families(kind):
        rows = []
        for cat, family in ((1, "Goods"), (2, "Weapon"), (3, "Protector"), (4, "Accessory")):
            layers = [slot(family + kind + suffix) for suffix in ("", "_dlc01", "_dlc02")]
            rows.append(f"({cat}, [{', '.join(map(str, layers))}])")
        return "[" + ", ".join(rows) + "]"

    return [
        "/// The item text files we use, by their file id in item.msgbnd (= MsgRepository slot).",
        f"pub const PLACE_NAME_SLOT: usize = {slot('PlaceName')};",
        "/// Per ItemLot category, the layers base / _dlc01 / _dlc02 of its descriptions and names.",
        f"pub const CAPTION_SLOTS: [(u8, [usize; 3]); 4] = {families('Caption')};",
        f"pub const NAME_SLOTS: [(u8, [usize; 3]); 4] = {families('Name')};",
        "",
    ]


def write_rust(classes, markers, placer, report):
    lines = [
        "// @generated by tools/generate.py from The Convergence's files. Do not edit by hand.",
        "// Positions are written to 3 decimals, more than an f32 holds for large values.",
        "#![allow(clippy::excessive_precision)]",
        "use crate::markers::{GenMarker, Item, Note};",
        "use crate::tracker::{NoteEntry, NoteSection, NoteText};",
        "",
    ] + text_slots() + [
        "/// Every class note, with the event flags its pickups set (owning the note = any flag on).",
        "pub const NOTES: &[Note] = &[",
    ]
    icons = goods_icons()
    for note, cls, which, flags in note_flags(classes):
        lines.append(f"    Note {{ goods_id: {note}, class: {rust_str(cls)}, which: {rust_str(which)}, "
                     f"flags: &[{', '.join(map(str, flags))}], icon: {icons.get(note, 0)} }},")
    lines += ["];", "", "pub const MARKERS: &[GenMarker] = &["]
    for m in markers:
        lines.append(
            "    GenMarker { note: %d, kind: %d, items: &[%s], block: %d, area: %d, grid_x: %d, grid_z: %d, "
            "x: %.3f, y: %.3f, z: %.3f, disp_mask: %d, text_id: %d, line: %d, name_item: %s, label: %s },"
            % (m["note"], m["kind"], ", ".join("Item { category: %d, id: %d, flag: %d }" % i for i in m["items"]),
               m["block"], m["area"], m["gx"], m["gz"],
               m["x"], m["y"], m["z"], m["disp_mask"], m["text_id"], m["line"], m["name_item"],
               rust_str(f'{m["cls"]}: note {m["note"]} line {m["line"]} ({m["src"]}, {m["how"]})')))
    lines += ["];", "",
              "/// Item descriptions to extend: (ItemLot category, item id, [goods ids of the notes",
              "/// listing it, one per class]).",
              "pub const CAPTIONS: &[(u8, u32, &[u32])] = &["]
    for (cat, item), listed in class_captions(classes):
        lines.append(f"    ({cat}, {item}, &[{', '.join(str(note) for _, note in listed)}]),")
    lines += ["];", "",
              "/// The class notes, rebuilt with your progress (see tracker.rs).",
              "pub const NOTE_TEXTS: &[NoteText] = &["]
    item = lambda i: "Item { category: %d, id: %d, flag: %d }" % i
    for note, intro, sections in note_texts(classes, report):
        lines.append(f"    NoteText {{ note: {note}, intro_note: {intro}, sections: &[")
        for kind, source, entries in sections:
            lines.append(f"        NoteSection {{ kind: {kind}, source: {source}, entries: &[")
            for line, named, items, first in entries:
                lines.append(f"            NoteEntry {{ line: {line}, name_item: {named}, "
                             f"items: &[{', '.join(map(item, items))}], new_group: {str(first).lower()} }},")
            lines.append("        ] },")
        lines.append("    ] },")
    lines += ["];", ""]
    lines += ["/// Spell rune tiers' map icons: (class, tier, icon id of the rune's detail image).",
              "pub const RUNE_ART: &[(&str, &str, u16)] = &["]
    lines += [f"    ({rust_str(c)}, {rust_str(t)}, {i})," for c, t, i in rune_art(classes)]
    lines += ["];", ""]
    open(common.GENERATED_RS, "w", encoding="utf-8").write("\n".join(lines))


def write_matching_report(classes, report):
    """matching_report.md: every note entry, what it matched and how, and whether it got a marker."""
    from matching import resolve
    from locate import note_entries
    placed = {(e["class"], e["name"]) for e in report if e["status"] in ("exact", "boss-marker", "place-name")}
    lines = ["# Class note matching report", "",
             "Generated by `tools/generate.py`. Every entry in the class notes, the item(s) it was",
             "matched to, and how. Anything not matched **exactly** is worth a look: it is usually a",
             "spelling difference between the note and the item's real name.", "",
             "| Class | Section | Note says | Matched item(s) | How | Marker |",
             "|---|---|---|---|---|---|"]
    corrections, sets = [], []
    for c in classes:
        for kind, name, _, siblings in note_entries(c):
            r = resolve(kind, name, siblings)
            items = ", ".join(sorted({n for n, _, _ in r.items})) or "**none**"
            how = r.how if r.how in ("exact", "set") else f"**{r.how}**"
            marker = "yes" if (c["name"], name) in placed else "no"
            lines.append(f"| {c['name']} | {kind} | {name} | {items} | {how} | {marker} |")
            if r.how.startswith("set"):
                sets.append(f"| {c['name']} | {name} | {len({n for n, _, _ in r.items})} | {items} | {r.how} |")
            elif r.how != "exact":
                corrections.append(f"| {c['name']} | {kind} | {name} | {items} | {r.how} | {marker} |")
    open(os.path.join(common.ROOT, "matching_report.md"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

    review = ["# Class note matching: everything that isn't an exact match", "",
              "Generated by `tools/generate.py`. The full list is in `matching_report.md`.", "",
              f"## Name corrections ({len(corrections)})", "",
              "The note's name differs from the item's real name; the item was found by the rule in *How*.", "",
              "| Class | Section | Note says | Matched item(s) | How | Marker |",
              "|---|---|---|---|---|---|"] + corrections + [
              "", f"## Armor sets ({len(sets)})", "",
              "\"… Set\" entries: one piece is found by name, the rest come from the same armor id block.", "",
              "| Class | Note says | Pieces | Matched pieces | How |",
              "|---|---|---|---|---|"] + sets

    def why_not_placed(e):
        hows = {r["how"] for r in e["results"]}
        if "shop-only" in hows:
            return "sold in a shop that isn't on the map (Roundtable Hold) or by an untraced merchant"
        if not any("lot" in r for r in e["results"]):
            return "no drop or shop source (remembrance trade, crafted or quest reward)"
        if "common-enemy" in hows:
            return "dropped by a common enemy / recurring NPC"
        if re.search(r"given by|quest", e["loc"], re.I):
            return "NPC quest or gift"
        return "drop source not traced (likely an unmodified base-game event script)"
    missing = [e for e in report if e["status"] not in ("exact", "boss-marker", "place-name")]
    review += ["", f"## Not placed ({len(missing)})", "",
               "Entries matched to an item but without a map position yet.", "",
               "| Class | Section | Note says | Location in note | Why |", "|---|---|---|---|---|"]
    review += [f"| {e['class']} | {e['kind']} | {e['name']} | {e['loc']} | {why_not_placed(e)} |" for e in missing]
    open(os.path.join(common.ROOT, "matching_review.md"), "w", encoding="utf-8").write("\n".join(review) + "\n")


def main():
    common.require_mod()
    extract()
    from build_data import build_classes
    classes = build_classes()
    import locate
    report = locate.locate_all(classes)
    placer = Placer(world_map_rows())
    markers, unplaced = build_markers(classes, report, placer)
    write_rust(classes, markers, placer, report)
    write_matching_report(classes, report)
    by_class = Counter(m["cls"] for m in markers)
    print(f"Wrote {len(markers)} markers for {len(by_class)} classes to {common.GENERATED_RS}")
    print(f"{len(unplaced)} note entries could not be placed (see cache/unplaced.json)")
    json.dump([{k: e[k] for k in ("class", "kind", "name", "loc", "status")} for e in unplaced],
              open(os.path.join(common.CACHE, "unplaced.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
