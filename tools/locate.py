"""Resolve every class-note item to map positions, using only the mod's own files."""
import glob, json, os, re, struct, sys
from collections import defaultdict

import common
from dump_fmg import dcx
from param import rows

MOD = common.MOD.replace("\\", "/")
MSB_DIR = MOD + "/map/MapStudio/"


def resolve_ids(kind, name, siblings=()):
    """Note entry -> [(item name, item id, ItemLot category)] (see matching.py)."""
    from matching import resolve
    return resolve(kind, name, siblings).items


def note_entries(c):
    """(kind, name, location text, sibling names) for every entry of a class's notes."""
    return [("rune", n, l, ()) for n, l in c["runes"]] +            [("armor", n, l, tuple(o for o, _ in g if o != n)) for g in c["armor"] for n, l in g] +            [("weapon", n, l, ()) for n, l in c["weapons"]]


# ---- item lots -------------------------------------------------------------
LOTS = defaultdict(list)  # (cat, id) -> [(param, lotId, pickupFlag)]
LOT_IDS = {}
for p in ["ItemLotParam_map", "ItemLotParam_enemy"]:
    r, _ = rows(f"{common.REG_DIR}/{p}.param")
    LOT_IDS[p] = set(r)
    for rid, b in r.items():
        ids = struct.unpack_from("<8i", b, 0)
        cats = struct.unpack_from("<8i", b, 0x20)
        flags = struct.unpack_from("<8i", b, 0x60)
        gf = struct.unpack_from("<i", b, 0x80)[0]
        for i, (x, c) in enumerate(zip(ids, cats)):
            if x:
                LOTS[(c, x)].append((p, rid, gf or flags[i]))


# ---- map files --------------------------------------------------------------
_msb_cache = {}


def msb(mapname):
    if mapname in _msb_cache:
        return _msb_cache[mapname]
    path = MSB_DIR + mapname + ".msb.dcx"
    if not os.path.exists(path):
        _msb_cache[mapname] = None
        return None
    d = dcx(open(path, "rb").read())

    def wstr(o):
        e = o
        while d[e:e + 2] != b"\0\0":
            e += 2
        return d[o:e].decode("utf-16-le")

    off, lists = 16, {}
    for _ in range(6):
        ver, cnt, nameoff = struct.unpack_from("<iiq", d, off)
        offs = struct.unpack_from(f"<{cnt}q", d, off + 16)
        lists[wstr(nameoff)] = offs[:-1]
        off = offs[-1]
    parts, by_entity = [], {}
    for o in lists["PARTS_PARAM_ST"]:
        no, inst, typ = struct.unpack_from("<qii", d, o)
        pos = struct.unpack_from("<3f", d, o + 0x20)
        ent_off = struct.unpack_from("<q", d, o + 0x60)[0]
        ent = struct.unpack_from("<i", d, o + ent_off)[0] if ent_off else 0
        npc = talk = 0
        if typ == 2:  # enemy: NpcParam id at +0x0C of the part's type data, talk script id at +0x10
            td = struct.unpack_from("<q", d, o + 0x68)[0]
            if td:
                npc, talk = struct.unpack_from("<ii", d, o + td + 0x0C)
        part = {"name": wstr(o + no), "type": typ, "pos": pos, "entity": ent, "npc": npc, "talk": talk}
        parts.append(part)
        if ent > 0:
            by_entity[ent] = part
    treasure = defaultdict(list)
    for o in lists["EVENT_PARAM_ST"]:
        no, eid, typ, idx = struct.unpack_from("<qiii", d, o)
        if typ == 4:
            bdo, tdo = struct.unpack_from("<qq", d, o + 0x18)
            pi = struct.unpack_from("<i", d, o + tdo + 8)[0]
            lot = struct.unpack_from("<i", d, o + tdo + 0x10)[0]
            if 0 <= pi < len(parts):
                treasure[lot].append(parts[pi])
    _msb_cache[mapname] = {"parts": parts, "entity": by_entity, "treasure": treasure}
    return _msb_cache[mapname]


ALL_MAPS = sorted(os.path.basename(f)[:-8] for f in glob.glob(MSB_DIR + "*.msb.dcx"))


_world_index = None
INDEX_VERSION = 2   # bump when the index gains fields


def world_index():
    """Across ALL map files: placed enemies by NpcParam id, pickups by item lot and NPCs by talk
    script id. {"enemies": {npc: [(map, part)]}, "pickups": {lot: [...]}, "talks": {talk: [...]}}.
    Cached in cache/ and rebuilt when the map files change."""
    global _world_index
    if _world_index is not None:
        return _world_index
    files = glob.glob(MSB_DIR + "*.msb.dcx")
    stamp = [len(files), max(int(os.path.getmtime(f)) for f in files), INDEX_VERSION]
    path = os.path.join(common.CACHE, "world_index.json")
    if os.path.exists(path):
        data = json.load(open(path, encoding="utf-8"))
        if data.get("stamp") == stamp:
            _world_index = {k: {int(i): v for i, v in data[k].items()} for k in ("enemies", "pickups", "talks")}
            return _world_index
    print("Indexing enemies and pickups in all map files (once) ...")
    enemies, pickups, talks = defaultdict(list), defaultdict(list), defaultdict(list)
    for m in ALL_MAPS:
        ms = msb(m)
        if not ms:
            continue
        for p in ms["parts"]:
            if p["npc"] > 0:
                enemies[p["npc"]].append((m, p))
            if p["talk"] > 0:
                talks[p["talk"]].append((m, p))
        for lot, ps in ms["treasure"].items():
            for p in ps:
                pickups[lot].append((m, p))
        _msb_cache.pop(m, None)  # keep memory flat
    json.dump({"stamp": stamp, "enemies": enemies, "pickups": pickups, "talks": talks},
              open(path, "w", encoding="utf-8"))
    _world_index = {"enemies": dict(enemies), "pickups": dict(pickups), "talks": dict(talks)}
    return _world_index


# NpcParam -> the item lots its enemies drop (itemLotId fields at +0x30 / +0x34).
NPC_BY_LOT = defaultdict(list)
_npc_rows, _ = rows(f"{common.REG_DIR}/NpcParam.param")
for _npc, _b in _npc_rows.items():
    for _at in (0x30, 0x34):
        _lot = struct.unpack_from("<i", _b, _at)[0]
        if _lot > 0:
            NPC_BY_LOT[_lot].append(_npc)
del _npc_rows

# A drop whose enemy is placed in more distinct spots than this is a common enemy or a recurring
# NPC (D, Juno Hoslow appear along their questlines), not a specific location: no marker.
MAX_ENEMY_SPOTS = 3

# Result kinds that carry a map position.
PLACED = ("pickup", "event", "defeat", "enemy", "shop-unlock", "merchant")


def map_for_id(n):
    """Guess the map a lot/entity ID belongs to from its digits."""
    s = str(n)
    if len(s) == 10 and s[:2] in ("10", "20"):
        cand = f"m6{int(s[0]) - 1}_{s[2:4]}_{s[4:6]}_00"
    elif len(s) == 8:
        cand = f"m{s[0:2]}_{s[2:4]}_00_00"
    else:
        return None
    return cand if cand in ALL_MAPS else None


# ---- event scripts ----------------------------------------------------------
EMEVD = {os.path.basename(f).split(".")[0]: open(f, encoding="utf-8", errors="replace").read()
         for f in glob.glob(MOD + "/event/*.emevd.dcx.js")}


import functools
@functools.lru_cache(None)
def lot_in_events(lot):
    """Return [(script map, line, numbers on the line)] mentioning this lot."""
    out = []
    rx = re.compile(r"(?<![\d.])%d(?![\d.])" % lot)
    for m, text in EMEVD.items():
        for line in text.splitlines():
            if rx.search(line) and "DisplayGenericDialog" not in line:
                code = line.split("//")[0]
                # InitializeEvent(slot, eventId, args...): only the args are game values
                code = re.sub(r"Initialize(?:Common)?Event\(\s*\d+\s*,\s*\d+\s*,?", "(", code)
                nums = [int(x) for x in re.findall(r"(?<![\w.])\d{4,10}(?![\w.])", code) if int(x) != lot]
                out.append((m, line.strip(), nums))
    return out


EVENT_BODIES = []  # (script map, body text)
for _m, _text in EMEVD.items():
    for _body in re.split(r"\n(?=\$Event\()", _text):
        EVENT_BODIES.append((_m, _body))


@functools.lru_cache(None)
def flag_source(flag):
    """Where in the world does this flag get turned on? -> (map, part) or None.

    1. An event that sets the flag after a boss dies: use that boss.
    2. Otherwise an enemy/NPC entity named next to the flag in that map's script.
    """
    rx = re.compile(r"SetEventFlagID\(%d, ON\)" % flag)
    for m, body in EVENT_BODIES:
        if rx.search(body) and m in ALL_MAPS:
            ms = msb(m)
            for e in re.findall(r"(?:HandleBossDefeat\w*|CharacterDead)\((\d+)", body):
                if ms and int(e) in ms["entity"]:
                    return m, ms["entity"][int(e)]
    rxf = re.compile(r"(?<![\d.])%d(?![\d.])" % flag)
    for m, text in EMEVD.items():
        if m not in ALL_MAPS:
            continue
        ms = msb(m)
        for line in text.splitlines():
            if rxf.search(line):
                for n in re.findall(r"(?<![\w.])\d{7,10}(?![\w.])", line.split("//")[0]):
                    p = ms["entity"].get(int(n)) if ms else None
                    if p and p["type"] == 2:  # enemy / NPC part
                        return m, p
    return None


# ---- the mod's boss markers -------------------------------------------------
PLACE = {}
for f in ["PlaceName", "PlaceName_dlc01", "PlaceName_dlc02"]:
    PLACE.update({int(k): v for k, v in json.load(open(f"{common.ITEM_DIR}/{f}.fmg.json", encoding="utf-8")).items()})
WMP, _ = rows(f"{common.REG_DIR}/WorldMapPointParam.param")
BOSS_MARKERS = []
for rid, b in WMP.items():
    t = struct.unpack_from("<i", b, 0x30)[0]
    if b[0x0C:0x0E] == b"\xf3\x01" and t in PLACE:  # icon 499 = the mod's boss icon
        area, gx, gz = b[0x20], b[0x21], b[0x22]
        BOSS_MARKERS.append({"row": rid, "boss": PLACE[t], "area": area, "gx": gx, "gz": gz,
                             "pos": struct.unpack_from("<3f", b, 0x24)})


# Named map icons a note's location text can refer to: Convergence's boss markers and every
# landmark icon with a place name (evergaols, caves, ruins...). Region labels (icon 400) are left
# out: they name whole areas, not a place to go.
REGION_LABEL_ICON = 400


def _world_xz(b):
    """(area, world x, world z) of a row; overworld rows are tile + local position."""
    area, gx, gz = b[0x20], b[0x21], b[0x22]
    x, z = struct.unpack_from("<f", b, 0x24)[0], struct.unpack_from("<f", b, 0x2C)[0]
    return (area, gx * 256 + x, gz * 256 + z) if area in (60, 61) else (area, x, z)


NAMED_ROWS, REGION_LABELS = [], []
for rid, b in WMP.items():
    t = struct.unpack_from("<i", b, 0x30)[0]
    icon = struct.unpack_from("<H", b, 0x0C)[0]
    if t not in PLACE or not PLACE[t] or "<" in PLACE[t]:
        continue
    if icon == REGION_LABEL_ICON:
        REGION_LABELS.append({"name": PLACE[t], "at": _world_xz(b)})
    else:
        NAMED_ROWS.append({"row": rid, "name": PLACE[t], "boss": icon == 499, "at": _world_xz(b)})


def region_words(row):
    """Words of the region label nearest to a row ("East Caelid - Dragonbarrow"), same area only."""
    from matching import norm
    area, x, z = row["at"]
    near = [r for r in REGION_LABELS if r["at"][0] == area]
    if not near:
        return set()
    best = min(near, key=lambda r: (r["at"][1] - x) ** 2 + (r["at"][2] - z) ** 2)
    return set(norm(best["name"]))


def place_for_text(loc):
    """The named map icon(s) a note's location text refers to: the longest icon name contained in
    it (spelling-insensitive, whole words), e.g. "Forlorn Hound Evergaol", "Found after defeating
    Valiant Gargoyle in Altus". Boss markers win ties when the text talks about a defeat/drop.
    When the name exists several times (eight Night's Cavalry), the copies whose region shares
    the most words with the note win ("East Liurnia Night's Cavalry Drop"); all if none does.
    NPC gifts and quests name a condition, not a place, so they get nothing here."""
    from matching import norm
    if re.search(r"given by|quest", loc, re.I):
        return []
    words = norm(loc)
    defeat = re.search(r"defeat|drop|killed|slain", loc, re.I) is not None
    best, hits = None, []
    for r in NAMED_ROWS:
        nw = norm(r["name"])
        if not nw or not any(words[i:i + len(nw)] == nw for i in range(len(words) - len(nw) + 1)):
            continue
        score = (len(nw), r["boss"] == defeat)
        if best is None or score > best:
            best, hits = score, [r]
        elif score == best:
            hits.append(r)
    same_name = [r for r in hits if r["name"] == hits[0]["name"]] if hits else []
    if len(same_name) > 1:
        # Area names decide, compass words only break ties ("East Liurnia" is not East Caelid).
        compass = {"north", "south", "east", "west", "central"}
        overlap = {r["row"]: (len((region_words(r) - compass) & set(words)),
                              len(region_words(r) & compass & set(words))) for r in same_name}
        top = max(overlap.values())
        if top[0] == 0:
            return []  # the note doesn't say which one: no marker beats a pile of wrong ones
        same_name = [r for r in same_name if overlap[r["row"]] == top]
    return same_name


# ---- shops ------------------------------------------------------------------
# ShopLineupParam: equipId +0x00, mtrlId +0x08 (-1 = bought with runes), eventFlag_forStock +0x0C
# (on once a limited item is sold out), eventFlag_forRelease +0x10 (the row is for sale once it's
# on), sellQuantity +0x14 (-1 = unlimited), equipType +0x17.
SHOP_EQUIP_TYPE = {2: 0, 3: 1, 4: 2, 1: 3, 5: 4}    # ItemLot category -> equipType
SHOP_ROWS = []   # (row id, item category, item id, stock flag, release flag)
_shop_rows, _ = rows(f"{common.REG_DIR}/ShopLineupParam.param")
_cat_of = {t: c for c, t in SHOP_EQUIP_TYPE.items()}
for _rid, _b in _shop_rows.items():
    _eq, _, _mtrl, _stock, _release = struct.unpack_from("<iiiII", _b, 0)
    _qty = struct.unpack_from("<h", _b, 0x14)[0]
    if _mtrl == -1 and _b[0x17] in _cat_of:   # material trades and crafting are not shop stock
        SHOP_ROWS.append((_rid, _cat_of[_b[0x17]], _eq, _stock if _qty > 0 else 0, _release))

TALK_DIR = MOD + "/script/talk/"
# Merchant ranges: a talk script opens its shop with the first and last row id as two
# consecutive integer constants (bytecode 0x82 <i32> 0xA1).
_RANGE = re.compile(rb"\x82(.{4})\xa1(?=\x82(.{4})\xa1)", re.S)   # overlapping pairs


@functools.lru_cache(None)
def shop_ranges():
    """[(first row, last row, talk script id)] from Convergence's talk scripts. Only the scripts
    Convergence ships are readable; unmodified base-game merchants are missing."""
    from dump_fmg import bnd4
    out = []
    for f in glob.glob(TALK_DIR + "*.talkesdbnd.dcx"):
        for _, name, data in bnd4(dcx(open(f, "rb").read())):
            talk = int(re.search(r"t(\d+)\.esd", name).group(1))
            for m in _RANGE.finditer(data):
                a, b = struct.unpack("<i", m.group(1))[0], struct.unpack("<i", m.group(2))[0]
                if 100000 <= a < b <= a + 100:
                    out.append((a, b, talk))
    return out


# Merchants whose map isn't on the world map (Roundtable Hold) can't get a marker.
OFF_MAP = ("m11_10_",)
# Wandering merchants (Patches) sell the same stock in several places; more than this many is
# a shared stock list, not a place to go.
MAX_MERCHANT_SPOTS = 5


def locate_shop(cat, item_id):
    """Shop stock for this item: at the boss whose defeat puts it up for sale (Enia's
    remembrance and armor shops), else at the merchant who sells it."""
    results = []
    for rid, c, eq, stock, release in SHOP_ROWS:
        if c != cat or eq != item_id:
            continue
        src = flag_source(release) if release else None
        if src:
            m, p = src
            results.append({"how": "shop-unlock", "map": m, "pos": p["pos"], "part": p["name"],
                            "shop": rid, "flag": stock, "trigger": release})
            continue
        sellers = [mp for a, b, talk in shop_ranges() if a <= rid <= b
                   for mp in world_index()["talks"].get(talk, []) if not mp[0].startswith(OFF_MAP)]
        spots = {(m[:9], round(p["pos"][0]), round(p["pos"][2])) for m, p in sellers}   # m60_47_46_10 = _00
        if 0 < len(spots) <= MAX_MERCHANT_SPOTS:
            for m, p in sellers:
                results.append({"how": "merchant", "map": m, "pos": p["pos"], "part": p["name"],
                                "shop": rid, "flag": stock})
        else:
            results.append({"how": "shop-only", "shop": rid, "flag": stock, "sellers": len(spots)})
    return results


# ---- resolution -------------------------------------------------------------
def locate(cat, item_id, loc_text):
    results = []
    for param, own_lot, flag in LOTS.get((cat, item_id), []):
        # A drop can be the tail of a chain: the game also awards rows id+1, id+2...
        # with the head. Try the lot itself, then each consecutive row below it.
        chain = [own_lot]
        while len(chain) < 10 and chain[-1] - 1 in LOT_IDS[param]:
            chain.append(chain[-1] - 1)
        found = False
        for lot in chain:
            # 1. a pickup placed in a map file
            for m in [map_for_id(lot)] if map_for_id(lot) else []:
                ms = msb(m)
                for part in (ms["treasure"].get(lot, []) if ms else []):
                    results.append({"how": "pickup", "map": m, "pos": part["pos"], "part": part["name"], "lot": lot, "flag": flag})
                    found = True
            if found:
                break
        if found:
            continue
        lot = next((l for l in chain if lot_in_events(l)), own_lot)
        # 2. an event script that hands out the lot, tied to an entity in a map
        for script, line, nums in lot_in_events(lot):
            for n in nums:
                m = map_for_id(n) or (script if script in ALL_MAPS else None)
                ms = msb(m) if m else None
                if ms and n in ms["entity"]:
                    p = ms["entity"][n]
                    results.append({"how": "event", "map": m, "pos": p["pos"], "part": p["name"], "lot": lot, "flag": flag})
                    found = True
                    break
            if found:
                break
        if not found:
            # 3. the lot is handed out when a flag turns on (boss/NPC defeated)
            for script, line, nums in lot_in_events(lot):
                for n in nums:
                    if n == lot:
                        continue
                    src = flag_source(n)
                    if src:
                        m, p = src
                        results.append({"how": "defeat", "map": m, "pos": p["pos"], "part": p["name"], "lot": lot, "flag": flag, "trigger": n})
                        found = True
                        break
                if found:
                    break
        if not found:
            # 4. a pickup in a map file other than the one the lot id points at
            for l in chain:
                for m, p in world_index()["pickups"].get(l, []):
                    results.append({"how": "pickup", "map": m, "pos": p["pos"], "part": p["name"], "lot": l, "flag": flag})
                    found = True
                if found:
                    break
        if not found:
            # 5. dropped by an enemy: lot <- NpcParam <- where that enemy is placed
            for l in chain:
                placed = [mp for npc in NPC_BY_LOT.get(l, []) for mp in world_index()["enemies"].get(npc, [])]
                if not placed:
                    continue
                spots = {(m, round(p["pos"][0]), round(p["pos"][2])) for m, p in placed}
                if len(spots) > MAX_ENEMY_SPOTS:
                    results.append({"how": "common-enemy", "lot": l, "flag": flag, "count": len(spots)})
                else:
                    for m, p in placed:
                        results.append({"how": "enemy", "map": m, "pos": p["pos"], "part": p["name"], "lot": l, "flag": flag})
                found = True
                break
        if not found:
            results.append({"how": "lot-only", "lot": lot, "flag": flag,
                            "events": [s for s, _, _ in lot_in_events(lot)][:3]})
    # 6. sold in a shop
    if not any(r["how"] in PLACED for r in results):
        results += locate_shop(cat, item_id)
    # 7. the note's location text names a boss marker or landmark icon on the map
    if not any(r["how"] in PLACED for r in results):
        for r in place_for_text(loc_text):
            results.append({"how": "boss-marker" if r["boss"] else "place-name", "row": r["row"], "boss": r["name"]})
    return results


def locate_all(classes, only=None):
    """Locate every item in every class note. Writes cache/locate_report.json."""
    report = []
    for c in classes:
        if only and c["id"] != only:
            continue
        for kind, name, loc, siblings in note_entries(c):
            ids = resolve_ids(kind, name, siblings)
            res = []
            for piece, i, cat in ids:
                for r in locate(cat, i, loc):
                    r.update(piece=piece, item_id=i, cat=cat)
                    res.append(r)
            hows = {r["how"] for r in res}
            how = ("unresolved-name" if not ids else
                   "exact" if hows & set(PLACED) else
                   "boss-marker" if "boss-marker" in hows else
                   "place-name" if "place-name" in hows else "no-position")
            report.append({"class": c["name"], "kind": kind, "name": name, "loc": loc, "status": how, "results": res})
    json.dump(report, open(f"{common.CACHE}/locate_report.json", "w", encoding="utf-8"),
              ensure_ascii=False, indent=1, default=list)
    return report


if __name__ == "__main__":
    from collections import Counter
    classes = json.load(open(f"{common.CACHE}/classes.json", encoding="utf-8"))
    report = locate_all(classes, sys.argv[1] if len(sys.argv) > 1 else None)
    print(Counter(r["status"] for r in report))
    for r in report:
        if r["status"] != "exact":
            print(f'{r["status"]:15} {r["class"]:18} {r["kind"]:6} {r["name"]} | {r["loc"]}')
