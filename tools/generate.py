"""Generate the Class Markers plugin's marker data from The Convergence's own files.

    python generate.py

1. Extracts regulation.bin params and the English item text from ConvergenceER\\mod into
   tools\\cache (skipped when the sources haven't changed since the last run).
2. Parses the class notes into per-class item lists.
3. Locates every listed item (pickups, boss/NPC rewards) in the map and event files.
4. Writes plugin\\src\\generated.rs, then the plugin is rebuilt with `cargo build --release`.
"""
import json
import re
import os
import struct
import sys
from collections import Counter, defaultdict

import common

# Map areas whose items are placed on the dungeon's entrance icon instead of their exact spot:
# caves, catacombs, tunnels, gaols and the like, which aren't drawn on the world map.
ENTRANCE_AREAS = {30, 31, 32, 40, 41, 42, 43}
OVERWORLD_AREAS = {60, 61}
BOSS_ICON = 499           # Convergence's boss marker icon; never used as an entrance
MARKER_ICON = 88          # round seal (user choice), see available_map_icons.png
ITEM_KIND = {"rune": 1, "weapon": 2, "armor": 3}   # ItemLot category numbers


def extract():
    """Decode Convergence's regulation.bin and English item text into the cache."""
    from dump_fmg import extract_msgbnd
    from reg import extract_regulation

    sources = [
        os.path.join(common.MOD, "regulation.bin"),
        os.path.join(common.MOD, "msg", "engus", "item_dlc02.msgbnd.dcx"),
    ]
    stamp_path = os.path.join(common.CACHE, "sources.json")
    stamp = {s: [os.path.getsize(s), int(os.path.getmtime(s))] for s in sources}
    if os.path.exists(stamp_path) and json.load(open(stamp_path)) == {k: v for k, v in stamp.items()}:
        print("Convergence files unchanged since the last run - using the cache.")
        return
    os.makedirs(common.CACHE, exist_ok=True)
    print("Extracting regulation.bin ...")
    extract_regulation(sources[0], common.REG_DIR)
    print("Extracting item text ...")
    extract_msgbnd(sources[1], common.ITEM_DIR)
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


class Placer:
    """Turns an item location (map + position) into a world-map row placement.

    A placement is (block, area, gx, gz, x, y, z, disp_mask, text_id). The row id the plugin
    gives it is block * 100 + a free two-digit number, because the map only draws rows whose
    id encodes their map block: 6 digits AA BB NN for dungeon maps (mAA_BB), 8 digits
    PP GG ZZ NN for world-map tiles. Verified in-game with test build 0.1.6.
    """

    def __init__(self, wmp):
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
        PL = {}
        for f in ["PlaceName", "PlaceName_dlc01", "PlaceName_dlc02"]:
            p = os.path.join(common.ITEM_DIR, f + ".fmg.json")
            PL.update({int(k): v for k, v in json.load(open(p, encoding="utf-8")).items()})
        self.place_names = PL

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

    def place_on_row(self, rid):
        r = self.rows[rid]
        return dict(block=rid // 100, area=r["area"], gx=r["gx"], gz=r["gz"], x=r["pos"][0],
                    y=r["pos"][1], z=r["pos"][2], disp_mask=r["disp_mask"], text_id=r["text_id"],
                    how="boss-marker")


def build_markers(classes, report, placer):
    out, unplaced = [], []
    # A marker belongs to the note that lists it: runes and armor are in "Path of the <class>",
    # weapons in "Weapons for <class>".
    path_note = {c["name"]: c["pathId"] for c in classes}
    weapons_note = {c["name"]: c.get("weaponsId") for c in classes}
    for entry in report:
        placed_any = False
        for res in entry["results"]:
            if res["how"] in ("pickup", "event", "defeat", "enemy"):
                p = placer.place(res["map"], res["pos"])
            elif res["how"] in ("boss-marker", "place-name"):
                p = placer.place_on_row(res["row"])
                p["how"] = res["how"]
            else:
                continue
            note = weapons_note[entry["class"]] if entry["kind"] == "weapon" else path_note[entry["class"]]
            out.append(dict(p, cls=entry["class"], note=note, entry=entry["name"],
                            kind=ITEM_KIND[entry["kind"]], item_id=res["item_id"], item=res["piece"],
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

    # One marker per note entry per spot: an armor set's pieces usually lie together.
    seen, unique = set(), []
    for m in out:
        k = (m["cls"], m["entry"], m["block"], round(m["x"]), round(m["z"]))
        if k not in seen:
            seen.add(k)
            unique.append(m)
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


def write_rust(classes, markers, placer):
    lines = [
        "// @generated by tools/generate.py from The Convergence's files. Do not edit by hand.",
        "use crate::markers::{GenMarker, Note};",
        "",
        "/// Every class note, with the event flags its pickups set (owning the note = any flag on).",
        "pub const NOTES: &[Note] = &[",
    ]
    for note, cls, which, flags in note_flags(classes):
        lines.append(f"    Note {{ goods_id: {note}, class: {rust_str(cls)}, which: {rust_str(which)}, "
                     f"flags: &[{', '.join(map(str, flags))}] }},")
    lines += ["];", "", "pub const MARKERS: &[GenMarker] = &["]
    for m in markers:
        lines.append(
            "    GenMarker { note: %d, kind: %d, item_id: %d, block: %d, area: %d, grid_x: %d, grid_z: %d, "
            "x: %.3f, y: %.3f, z: %.3f, disp_mask: %d, text_id: %d, hide_flag: %d, name: %s, label: %s },"
            % (m["note"], m["kind"], m["item_id"], m["block"], m["area"], m["gx"], m["gz"],
               m["x"], m["y"], m["z"], m["disp_mask"], m["text_id"], m["flag"], rust_str(m["entry"]),
               rust_str(f'{m["cls"]}: {m["entry"]} ({m["src"]}, {m["how"]})')))
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
        if not any("lot" in r for r in e["results"]):
            return "no drop source (shop, remembrance, crafted or quest reward)"
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
    extract()
    from build_data import build_classes
    classes = build_classes()
    import locate
    report = locate.locate_all(classes)
    placer = Placer(world_map_rows())
    markers, unplaced = build_markers(classes, report, placer)
    write_rust(classes, markers, placer)
    write_matching_report(classes, report)
    by_class = Counter(m["cls"] for m in markers)
    print(f"Wrote {len(markers)} markers for {len(by_class)} classes to {common.GENERATED_RS}")
    print(f"{len(unplaced)} note entries could not be placed (see cache/unplaced.json)")
    json.dump([{k: e[k] for k in ("class", "kind", "name", "loc", "status")} for e in unplaced],
              open(os.path.join(common.CACHE, "unplaced.json"), "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
