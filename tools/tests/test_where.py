"""Naming map positions and checking items against their notes (where.py), on made-up places.

Run from the repo root: python -m unittest discover -s tools/tests
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from where import (Places, check_entry, compass, map_of_id, names_in_text, to_overworld,  # noqa: E402
                   world)


def place(name, kind, mapname, pos, home=None, region=None, **extra):
    return {"name": name, "kind": kind, "map": mapname, "home": home or mapname,
            "at": world(mapname, pos), "region": region, **extra}


# A catacomb (m30_01) whose grace is drawn inside it and whose icon sits at its entrance on tile
# m60_42_33; a church two tiles away; a region label.
PLACES = [
    place("Impaler's Catacombs", "grace", "m30_01_00_00", (-27.0, 50.0, -50.0), region="Weeping Peninsula"),
    place("Frenzied Burial Watchdog", "boss", "m30_01_00_00", (-27.0, 50.0, -50.0), region="Weeping Peninsula"),
    place("Impaler's Catacombs", "icon", "m60_42_33_00", (10.0, 0.0, 10.0), home="m30_01_00_00", row=300100),
    place("Church of Pilgrimage", "grace", "m60_44_33_00", (0.0, 0.0, 0.0), region="Weeping Peninsula"),
    place("Church of Pilgrimage", "icon", "m60_44_33_00", (5.0, 0.0, 0.0), row=61443300),
    place("South Limgrave - Weeping Peninsula", "region", "m60_43_33_00", (0.0, 0.0, 0.0), row=61433300),
    place("Fort Something", "icon", "m60_42_33_00", (50.0, 0.0, 0.0)),
]
# Stormveil (m10_00): its point (0, 0, 0) is the overworld's m60_40_40 (100, 0, 100).
CONV = {"m10_00": [((0.0, 0.0, 0.0), "m60_40_40_00", (100.0, 0.0, 100.0))]}


def entry(loc, *results):
    return {"loc": loc, "results": list(results)}


class Coordinates(unittest.TestCase):
    def test_overworld_tiles_join_up(self):
        self.assertEqual(world("m60_42_36_00", (1.0, 2.0, 3.0)), ("m60", 42 * 256 + 1.0, 2.0, 36 * 256 + 3.0))

    def test_low_detail_tiles_are_bigger(self):
        # _01 covers 2x2 small tiles: its centre is the middle of small tiles 26/27.
        self.assertEqual(world("m60_13_14_01", (0.0, 0.0, 0.0))[1], 13 * 512 + 128)
        self.assertEqual(world("m60_11_09_12", (0.0, 0.0, 0.0))[1], 11 * 1024 + 384)

    def test_dungeons_keep_their_own_coordinates(self):
        self.assertEqual(world("m30_01_00_00", (1.0, 2.0, 3.0)), ("m30_01", 1.0, 2.0, 3.0))

    def test_ids_name_their_map(self):
        self.assertEqual(map_of_id(30010030), "m30_01_00_00")
        self.assertEqual(map_of_id(310400), "m31_04_00_00")
        self.assertEqual(map_of_id(1042367000), "m60_42_36_00")
        self.assertEqual(map_of_id(2049417500), "m61_49_41_00")
        self.assertIsNone(map_of_id(61423600))     # an overworld map row, not a dungeon

    def test_compass(self):
        self.assertEqual(compass(0, 10), "north")
        self.assertEqual(compass(10, 0), "east")
        self.assertEqual(compass(-10, -10), "south-west")

    def test_dungeon_to_overworld(self):
        self.assertEqual(to_overworld(CONV, "m10_00_00_00", (5.0, 1.0, -5.0)), ("m60", 40 * 256 + 105.0, 1.0, 40 * 256 + 95.0))
        self.assertIsNone(to_overworld(CONV, "m30_01_00_00", (0.0, 0.0, 0.0)))


class NamesInText(unittest.TestCase):
    KEYS = [("impaler", "catacomb"), ("church", "of", "pilgrimage"), ("stormhill",), ("stormhill", "shack")]

    def test_spelling_insensitive(self):
        self.assertEqual(names_in_text("Impaler Catacombs", self.KEYS), [("impaler", "catacomb")])

    def test_longer_name_wins(self):
        self.assertEqual(names_in_text("Found in the Stormhill Shack", self.KEYS), [("stormhill", "shack")])

    def test_whole_words_only(self):
        self.assertEqual(names_in_text("Pilgrimage church", self.KEYS), [])


class CheckEntry(unittest.TestCase):
    def setUp(self):
        self.places = Places([dict(p) for p in PLACES], CONV)
        self.wmp_at = {p["row"]: (p["map"], p["at"]) for p in self.places.all if "row" in p}

    def verdict(self, e):
        return check_entry(self.places, e, self.wmp_at)["verdict"]

    def test_item_inside_the_named_dungeon(self):
        self.assertEqual(self.verdict(entry("Impaler Catacombs", {"map": "m30_01_00_00", "pos": [-30.0, 20.0, -150.0]})), "ok")

    def test_item_in_another_map(self):
        self.assertEqual(self.verdict(entry("Impaler Catacombs", {"map": "m10_00_00_00", "pos": [0.0, 0.0, 0.0]})), "far")

    def test_item_near_a_named_landmark(self):
        self.assertEqual(self.verdict(entry("Church of Pilgrimage", {"map": "m60_44_33_00", "pos": [100.0, 0.0, 0.0]})), "ok")
        self.assertEqual(self.verdict(entry("Church of Pilgrimage", {"map": "m60_40_33_00", "pos": [0.0, 0.0, 0.0]})), "far")

    def test_region_name(self):
        self.assertEqual(self.verdict(entry("Weeping Peninsula", {"map": "m60_43_33_00", "pos": [0.0, 0.0, 0.0]})), "ok")

    def test_dungeon_item_near_an_overworld_landmark(self):
        # The note names a landmark next to the catacomb's entrance; the item is inside.
        self.assertEqual(self.verdict(entry("Fort Something", {"map": "m30_01_00_00", "pos": [0.0, 0.0, 0.0]})), "ok")

    def test_npc_texts_are_not_checked(self):
        self.assertEqual(self.verdict(entry("Sold by the merchant at Church of Pilgrimage",
                                            {"map": "m10_00_00_00", "pos": [0.0, 0.0, 0.0]})), "unchecked")

    def test_marker_row_results(self):
        self.assertEqual(self.verdict(entry("Church of Pilgrimage", {"how": "place-name", "row": 61443300})), "ok")

    def test_nothing_placed(self):
        self.assertEqual(self.verdict(entry("Impaler Catacombs", {"how": "lot-only"})), "unplaced")


if __name__ == "__main__":
    unittest.main()
