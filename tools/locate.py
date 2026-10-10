"""Resolve every class-note item to map positions, using only the mod's own files."""
import glob, json, os, re, struct, sys
from collections import defaultdict

import common
from dump_fmg import dcx
from param import rows

MOD = common.MOD.replace("\\", "/")
MSB_DIR = MOD + "/map/MapStudio/"
CAT = {"rune": 1, "weapon": 2, "armor": 3}


def fmg_names(prefix):
    out = defaultdict(list)
    for f in sorted(glob.glob(f"{common.ITEM_DIR}/{prefix}*.fmg.json")):
        for k, v in json.load(open(f, encoding="utf-8")).items():
            if v and v.strip() and not v.startswith("[ERROR]"):
                out[v.strip()].append(int(k))
    return out


NAMES = {"rune": fmg_names("GoodsName"), "weapon": fmg_names("WeaponName"), "armor": fmg_names("ProtectorName")}


def resolve_ids(kind, name):
    """Note entry name -> list of (item name, id)."""
    table = NAMES[kind]
    if name in table:
        return [(name, i) for i in table[name]]
    if kind == "armor":
        m = re.match(r"Set of (.+)$", name)
        if m:
            pat = lambda n: n.endswith(" of " + m.group(1))
        else:
            base = re.sub(r"\s+Set$", "", name)
            pat = lambda n: n.startswith(base + " ")
        hits = [(n, i) for n, ids in table.items() if pat(n) and "(Altered)" not in n for i in ids]
        return hits
    return []


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
        part = {"name": wstr(o + no), "type": typ, "pos": pos, "entity": ent}
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


def boss_for_text(loc):
    hits = [m for m in BOSS_MARKERS if m["boss"] and m["boss"].lower() in loc.lower()]
    hits.sort(key=lambda m: -len(m["boss"]))
    return hits[:1]


# ---- resolution -------------------------------------------------------------
def locate(kind, item_id, loc_text):
    results = []
    for param, own_lot, flag in LOTS.get((CAT[kind], item_id), []):
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
            results.append({"how": "lot-only", "lot": lot, "flag": flag,
                            "events": [s for s, _, _ in lot_in_events(lot)][:3]})
    # 3. the note names a boss that has a Convergence marker
    if not any(r["how"] in ("pickup", "event", "defeat") for r in results):
        for bm in boss_for_text(loc_text):
            results.append({"how": "boss-marker", "row": bm["row"], "boss": bm["boss"]})
    return results


def locate_all(classes, only=None):
    """Locate every item in every class note. Writes cache/locate_report.json."""
    report = []
    for c in classes:
        if only and c["id"] != only:
            continue
        entries = [("rune", n, l) for n, l in c["runes"]] +                   [("armor", n, l) for g in c["armor"] for n, l in g] +                   [("weapon", n, l) for n, l in c["weapons"]]
        for kind, name, loc in entries:
            ids = resolve_ids(kind, name)
            res = []
            for piece, i in ids:
                for r in locate(kind, i, loc):
                    r["piece"] = piece
                    res.append(r)
            how = "unresolved-name" if not ids else                 ("exact" if any(r["how"] in ("pickup", "event", "defeat") for r in res) else
                 "boss-marker" if any(r["how"] == "boss-marker" for r in res) else "no-position")
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
