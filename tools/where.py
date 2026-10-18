"""Where is that? Names a map position from the game's own data, so a located item can be checked
against its note without knowing the maps by heart.

  python tools/where.py "Shimmering Rune of Frenzied Flame"   an item from the last locate run
  python tools/where.py m30_01_00_00 -34.6 23.1 -155.7         a map file and a position in it
  python tools/where.py --find impaler                          named places matching a text
  python tools/where.py --check                                 every note entry vs. its location

Reference points: every Site of Grace (BonfireWarpParam: name, region, the boss Convergence puts
there) and every named world map icon (WorldMapPointParam: boss markers, landmarks, region labels).
Each has a spot (where it is drawn: a cave's grace sits at its entrance on the overworld) and a
home (the map it belongs to: the cave), from the grace's entity id or the icon's row id.
--check reads cache/locate_report.json (written by generate.py) and lists the entries whose note
names a place that is nowhere near where the item was found; cache/where_check.json has them all.
"""
import json
import math
import os
import re
import struct
import sys
from collections import Counter

import common
from matching import norm
from param import rows

REGION_LABEL_ICON = 400
BOSS_ICON = 499
NEAR = 500.0          # metres: a named place this close to the item counts as "there"
OVERWORLD = ("m60", "m61")
# Notes naming a person, not a place: what they name is where the NPC first stands, if anywhere.
NPC_TEXT = re.compile(r"sold by|given by|quest|merchant|bought|purchase", re.I)


# ---- coordinates --------------------------------------------------------------
def map_name(area, a, b, c=0):
    return f"m{area:02d}_{a:02d}_{b:02d}_{c:02d}"


def map_of_id(n):
    """The map an entity or row id belongs to, from its digits: 6 digits AABBNN (a world map
    row of dungeon mAA_BB), 8 digits AABBNNNN (an entity of mAA_BB), 10 digits 1GGZZNNNNN /
    2GGZZNNNNN (an entity of overworld tile m60_GG_ZZ / m61_GG_ZZ). None otherwise."""
    s = str(n)
    if len(s) == 10 and s[:2] in ("10", "20"):
        return f"m6{int(s[0]) - 1}_{s[2:4]}_{s[4:6]}_00"
    if len(s) in (6, 8) and s[:2] not in ("60", "61") and 10 <= int(s[:2]) <= 49:
        return f"m{s[:2]}_{s[2:4]}_00_00"
    return None


def space(mapname):
    """Maps whose positions can be compared: one per overworld (base game, DLC), else the map
    itself (m30_01 = Impaler's Catacombs; its variants _00/_01 share coordinates)."""
    return mapname[:3] if mapname[:3] in OVERWORLD else mapname[:6]


def same_dungeon(a, b):
    """Two maps of one legacy dungeon (Shadow Keep is m21_00..02, Leyndell m11_00/05). The
    underground (m12) is several places."""
    return a[:3] == b[:3] and 10 <= int(a[1:3]) <= 29 and a[:3] != "m12"


def world(mapname, pos):
    """(space, x, y, z): overworld tiles become one coordinate system. A tile's position is
    relative to its centre; low-detail maps (_01: 2x2 tiles, _02: 4x4) are bigger tiles."""
    x, y, z = pos
    if mapname[:3] in OVERWORLD:
        level = int(mapname[11])          # _10, _12: a variant of level 0 / 2
        size = 256 << level
        off = (size - 256) / 2
        x += int(mapname[4:6]) * size + off
        z += int(mapname[7:9]) * size + off
    return space(mapname), x, y, z


def compass(dx, dz):
    """Direction of (dx, dz) on the map: +X is east, +Z is north."""
    if math.hypot(dx, dz) < 1:
        return "here"
    i = round(math.degrees(math.atan2(dx, dz)) / 45) % 8
    return ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"][i]


def describe(frm, to):
    """'87 m south-west, 20 m below' from world spot `frm` to `to` (same space)."""
    dx, dy, dz = to[1] - frm[1], to[2] - frm[2], to[3] - frm[3]
    s = f"{math.hypot(dx, dz):.0f} m {compass(dx, dz)}"
    if abs(dy) >= 5:
        s += f", {abs(dy):.0f} m {'above' if dy > 0 else 'below'}"
    return s


def dist(a, b):
    return math.dist(a[1:], b[1:]) if a and b and a[0] == b[0] else math.inf


def load_conversions():
    """WorldMapLegacyConvParam: pairs of matching points that put dungeon maps on the overworld
    map. {map[:6]: [(point in it, other map, the same point there)]}; the other map can be
    another dungeon (a chain). +0x04 source area/block, +0x08 point, +0x14 target, +0x18 point."""
    conv = {}
    for b in rows(f"{common.REG_DIR}/WorldMapLegacyConvParam.param")[0].values():
        src = map_name(b[0x04], b[0x05], b[0x06])
        conv.setdefault(src[:6], []).append((struct.unpack_from("<3f", b, 0x08), map_name(b[0x14], b[0x15], b[0x16]),
                                             struct.unpack_from("<3f", b, 0x18)))
    return conv


def to_overworld(conv, mapname, pos, hops=4):
    """Rough overworld spot of a dungeon position, through the nearest matching point (a few
    hundred metres off at worst; good for "near X", not for placing anything). None if unknown."""
    if mapname[:3] in OVERWORLD:
        return world(mapname, pos)
    pairs = conv.get(mapname[:6])
    if not pairs or not hops:
        return None
    here, there, at = min(pairs, key=lambda t: math.dist(t[0], pos))
    return to_overworld(conv, there, tuple(p - h + a for p, h, a in zip(pos, here, at)), hops - 1)


# ---- reference points ----------------------------------------------------------
def load_places():
    """[{"name", "kind", "map", "home", "at": world spot, "region"?, "row"?}] from the params.
    kind: grace, boss, icon (landmark), region (a region label icon)."""
    text = {}
    for f in ["PlaceName", "PlaceName_dlc01", "PlaceName_dlc02"]:
        p = f"{common.ITEM_DIR}/{f}.fmg.json"
        if os.path.exists(p):
            text.update({int(k): v for k, v in json.load(open(p, encoding="utf-8")).items()})

    def name(t):
        v = text.get(t)
        return v if v and "<" not in v else None

    places = []
    # Graces: +0x08 entity id, +0x14 region, +0x20 area/block, +0x24 position, +0x30 grace
    # name, +0x3C the boss Convergence names there.
    graces, _ = rows(f"{common.REG_DIR}/BonfireWarpParam.param")
    for rid, b in graces.items():
        grace = name(struct.unpack_from("<i", b, 0x30)[0])
        if not rid or not grace:
            continue
        m = map_name(*b[0x20:0x24])
        home = map_of_id(struct.unpack_from("<I", b, 0x08)[0]) or m
        at = world(m, struct.unpack_from("<3f", b, 0x24))
        region = name(struct.unpack_from("<i", b, 0x14)[0])
        places.append({"name": grace, "kind": "grace", "map": m, "home": home, "at": at, "region": region})
        boss = name(struct.unpack_from("<i", b, 0x3C)[0])
        if boss:
            places.append({"name": boss, "kind": "boss", "map": m, "home": home, "at": at, "region": region})
    # World map icons: up to 8 text lines (+0x30 + 12k), line type at +0x90 + k (0 = place name).
    wmp, _ = rows(f"{common.REG_DIR}/WorldMapPointParam.param")
    for rid, b in wmp.items():
        icon = struct.unpack_from("<H", b, 0x0C)[0]
        m = map_name(b[0x20], b[0x21], b[0x22])
        home = (map_of_id(rid) if len(str(rid)) == 6 else None) or m
        at = world(m, struct.unpack_from("<3f", b, 0x24))
        kind = "region" if icon == REGION_LABEL_ICON else "boss" if icon == BOSS_ICON else "icon"
        for k in range(8):
            n = name(struct.unpack_from("<i", b, 0x30 + 12 * k)[0]) if b[0x90 + k] == 0 else None
            if n:
                places.append({"name": n, "kind": kind, "map": m, "home": home, "at": at, "row": rid})
                break
    return places


def words(s):
    w = norm(s)
    return w[1:] if w[:1] == ["the"] and len(w) > 2 else w   # "The Shaded Castle" = "Shaded Castle"


class Places:
    def __init__(self, places, conv=None):
        """`conv`: load_conversions(); without it, dungeon spots have no overworld position."""
        self.conv = conv or {}
        # The graces' region names are places too (kind area, no spot): a note can name one.
        areas = sorted({p["region"] for p in places if p["kind"] == "grace" and p.get("region")})
        places = places + [{"name": a, "kind": "area", "map": None, "home": None, "at": None} for a in areas]
        for p in places:
            p.setdefault("ow", self.overworld(p["map"], p["at"]))
        self.all = places
        self.graces = [p for p in places if p["kind"] == "grace"]
        self.by_key = {}
        for p in places:
            self.by_key.setdefault(tuple(words(p["name"])), []).append(p)

    def overworld(self, mapname, at):
        """Overworld spot of a world spot (itself, or a dungeon spot converted)."""
        if not at or at[0] in OVERWORLD:
            return at
        return to_overworld(self.conv, mapname, at[1:])

    def inside(self, mapname):
        """Graces belonging to a dungeon map."""
        return [g for g in self.graces if space(g["home"]) == space(mapname)]

    def map_label(self, mapname):
        """'Impaler's Catacombs (Weeping Peninsula)' for a dungeon; None for the overworld."""
        if mapname[:3] in OVERWORLD:
            return None
        inside = self.inside(mapname)
        if not inside:
            icons = [p["name"] for p in self.all if p["kind"] == "icon" and space(p["home"]) == space(mapname)]
            return icons[0] if icons else None
        names = list(dict.fromkeys(g["name"] for g in inside))
        region = Counter(g["region"] for g in inside if g["region"]).most_common(1)
        region = region[0][0] if region else None
        if len(names) == 1:
            return f"{names[0]} ({region})" if region and region != names[0] else names[0]
        return f"{region} (graces: {', '.join(names[:4])}{', ...' if len(names) > 4 else ''})"

    def regions(self, mapname, at):
        """Region names of a spot: its dungeon's graces', else those of the 3 nearest graces and
        the nearest region label ("East Caelid - Dragonbarrow")."""
        if mapname[:3] not in OVERWORLD:
            return {g["region"] for g in self.inside(mapname) if g["region"]}
        if not at:
            return set()
        graces = sorted((g for g in self.graces if g["at"][0] == at[0] and g["region"]), key=lambda g: dist(g["at"], at))
        labels = [p for p in self.all if p["kind"] == "region" and p["at"][0] == at[0]]
        out = {g["region"] for g in graces[:3]}
        if labels:
            out.add(min(labels, key=lambda p: dist(p["at"], at))["name"])
        return out

    def nearby(self, at, n=6):
        same = [p for p in self.all if p["at"] and p["at"][0] == at[0] and p["kind"] != "region"]
        return sorted(same, key=lambda p: dist(p["at"], at))[:n]

    def entrances(self, mapname):
        """Spots on the overworld that belong to dungeon `mapname` (its icon, its graces)."""
        if mapname[:3] in OVERWORLD:
            return []
        return [p for p in self.all if p["home"] and space(p["home"]) == space(mapname)
                and p["at"] and p["at"][0] in OVERWORLD]


# ---- checking a note against a position ----------------------------------------------
def names_in_text(text, keys):
    """The place names (word tuples from `keys`) a text contains as whole words; a name inside a
    longer one that also matched ("Stormhill" in "Stormhill Shack") is dropped."""
    w = words(text)

    def has(seq, k):
        return any(tuple(seq[i:i + len(k)]) == k for i in range(len(seq) - len(k) + 1))
    hits = [k for k in keys if k and has(w, k)]
    return [k for k in hits if not any(len(o) > len(k) and has(list(o), k) for o in hits)]


def near_named(places, key, mapname, at):
    """Is a spot (map, world spot or None) at a place called `key`? -> (yes, why)."""
    named = places.by_key.get(key, [])
    dungeon = mapname[:3] not in OVERWORLD
    for p in named:
        if dungeon and p["home"] and (space(p["home"]) == space(mapname) or same_dungeon(p["home"], mapname)):
            return True, f"in {p['home'][:6]}"
    if any(set(key) <= set(words(r)) for r in places.regions(mapname, at)):
        return True, "in region"
    spots = ([at, places.overworld(mapname, at)] if at else []) + [e["at"] for e in places.entrances(mapname)]
    pairs = [(dist(q, s), p) for p in named if p["kind"] != "region" for q in (p["at"], p["ow"]) for s in spots]
    best = min(pairs, key=lambda t: t[0], default=(math.inf, None))
    if best[0] <= NEAR:
        return True, f"{best[0]:.0f} m"
    if best[0] < math.inf:
        return False, f"nearest is {best[0]:.0f} m away"
    homes = sorted({p["home"][:6] for p in named if p["home"]})
    return False, ("elsewhere: " + ", ".join(homes[:3])) if homes else "a region it isn't in"


def result_spot(r, wmp_at):
    """(map, world spot or None) of one locate result; a result without a map gives None."""
    if "pos" in r:
        return r["map"], world(r["map"], r["pos"])
    if "row" in r and r["row"] in wmp_at:
        return wmp_at[r["row"]]
    if "map" in r:
        return r["map"], (world(r["map"], (0.0, 0.0, 0.0)) if r["map"][:3] in OVERWORLD else None)
    return None


def check_entry(places, entry, wmp_at):
    """{"verdict": ok|far|unchecked|unplaced, "names", "details"} for a locate_report entry."""
    spots = list(dict.fromkeys(s for s in (result_spot(r, wmp_at) for r in entry["results"]) if s))
    if not spots:
        return {"verdict": "unplaced", "names": [], "details": []}
    keys = [] if NPC_TEXT.search(entry["loc"]) else names_in_text(entry["loc"], list(places.by_key))
    if not keys:
        return {"verdict": "unchecked", "names": [], "details": []}
    details = []
    for m, at in spots:   # several pieces at one spot: checked once
        res = [(k, *near_named(places, k, m, at)) for k in keys]
        good = [r for r in res if r[1]]
        details.append({"map": m, "ok": bool(good),
                        "matches": [(" ".join(k), why) for k, _, why in (good or res)]})
    return {"verdict": "ok" if all(d["ok"] for d in details) else "far",
            "names": [" ".join(k) for k in keys], "details": details}


def wmp_positions(places):
    return {p["row"]: (p["map"], p["at"]) for p in places.all if "row" in p}


def check_all(places, report):
    """Check every entry of a locate report; writes cache/where_check.json."""
    wmp_at = wmp_positions(places)
    out = [{k: e[k] for k in ("class", "kind", "name", "loc")} | check_entry(places, e, wmp_at) for e in report]
    json.dump(out, open(os.path.join(common.CACHE, "where_check.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    return out


# ---- command line --------------------------------------------------------------------
def print_spot(places, mapname, at, indent="  "):
    label = places.map_label(mapname)
    print(f"{indent}{mapname}" + (f"  {label}" if label else ""))
    if mapname[:3] in OVERWORLD:
        print(f"{indent}  region: {', '.join(places.regions(mapname, at)) or '?'}")
    if at is None:
        print(f"{indent}  (no position: somewhere in this map)")
    else:
        for p in places.nearby(at):
            print(f"{indent}  {p['kind']:6} {p['name']:40} {describe(at, p['at'])}")
    ow = places.overworld(mapname, at)
    if ow and ow is not at:
        print(f"{indent}  roughly on the overworld at ({ow[1]:.0f}, {ow[3]:.0f}) {ow[0]}, near:")
        for p in places.nearby(ow, 3):
            print(f"{indent}    {p['kind']:6} {p['name']:38} {describe(ow, p['at'])}")
    for e in places.entrances(mapname)[:1]:
        print(f"{indent}  on the overworld at {e['map']} ({e['name']})")


def main(argv):
    if not argv:
        print(__doc__)
        return
    places = Places(load_places(), load_conversions())
    if argv[0] == "--find":
        q = words(" ".join(argv[1:]))
        for p in places.all:
            if p["at"] and all(any(x in y for y in words(p["name"])) for x in q):
                print(f"{p['kind']:6} {p['name']:40} {p['home']}  "
                      f"(drawn in {p['map']} at {p['at'][1]:.0f}, {p['at'][2]:.0f}, {p['at'][3]:.0f})")
        return
    if re.fullmatch(r"m\d\d_\d\d_\d\d_\d\d", argv[0]) and len(argv) == 4:
        print_spot(places, argv[0], world(argv[0], tuple(float(v) for v in argv[1:])), indent="")
        return
    report = json.load(open(os.path.join(common.CACHE, "locate_report.json"), encoding="utf-8"))
    if argv[0] == "--check":
        out = check_all(places, report)
        print(Counter(e["verdict"] for e in out))
        for e in out:
            if e["verdict"] == "far":
                print(f"\nFAR  {e['class']} {e['kind']}: {e['name']}  | note: {e['loc']}")
                for d in e["details"]:
                    print(f"     {'ok ' if d['ok'] else 'far'} {d['map']} {places.map_label(d['map']) or ''}: "
                          + "; ".join(f"{n} - {why}" for n, why in d["matches"]))
        return
    q = " ".join(argv).lower()
    wmp_at = wmp_positions(places)
    hits = [e for e in report if q in e["name"].lower() or any(q in r.get("piece", "").lower() for r in e["results"])]
    for e in hits:
        c = check_entry(places, e, wmp_at)
        print(f"{e['class']} {e['kind']}: {e['name']}  [{c['verdict']}]")
        print(f"  note: {e['loc']}")
        for m, at in dict.fromkeys(s for s in (result_spot(r, wmp_at) for r in e["results"]) if s):
            print_spot(places, m, at, indent="  ")
    if not hits:
        print("No such item in cache/locate_report.json (run tools/generate.py first).")


if __name__ == "__main__":
    main(sys.argv[1:])
