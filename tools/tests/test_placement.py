"""Item location -> world map row placement (generate.py small_tile and Placer), on made-up rows.

Run from the repo root: python -m unittest discover -s tools/tests
"""
import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from generate import BOSS_ICON, Placer, small_tile  # noqa: E402


def row(area, gx, gz, pos=(0.0, 0.0, 0.0), icon=1, disp_mask=1, text_id=-1):
    """A WorldMapPointParam row with the fields Placer reads."""
    b = bytearray(256)
    struct.pack_into("<H", b, 0x0C, icon)
    b[0x18] = disp_mask
    b[0x20], b[0x21], b[0x22] = area, gx, gz
    struct.pack_into("<3f", b, 0x24, *pos)
    struct.pack_into("<i", b, 0x30, text_id)
    return bytes(b)


ROWS = {
    # Overworld tile m60_42_36: a named place (prefix 61 = Limgrave).
    61423600: row(60, 42, 36, pos=(10.0, 0.0, 20.0), text_id=1000),
    # A cave, m31_04: a boss marker first, then its entrance icon on the overworld.
    310400: row(60, 43, 37, pos=(1.0, 2.0, 3.0), icon=BOSS_ICON, text_id=2000),
    310401: row(60, 43, 37, pos=(5.0, 6.0, 7.0), icon=50, disp_mask=1, text_id=3000),
}
PLACE_NAMES = {1000: "Church of Elleh", 2000: "Some Boss", 3000: "Murkwater Cave"}


class SmallTile(unittest.TestCase):
    def test_regular_tile_is_unchanged(self):
        self.assertEqual(small_tile("m60_42_36_00", (1.0, 2.0, 3.0)), ("m60_42_36_00", (1.0, 2.0, 3.0)))

    def test_dungeon_is_unchanged(self):
        self.assertEqual(small_tile("m31_04_00_00", (1.0, 2.0, 3.0)), ("m31_04_00_00", (1.0, 2.0, 3.0)))

    def test_2x2_tile_becomes_the_regular_tile(self):
        # m60_10_10_01 covers tiles 20-21 x 20-21; its centre is the corner of tiles 20 and 21.
        self.assertEqual(small_tile("m60_10_10_01", (-100.0, 5.0, 50.0)), ("m60_20_21_00", (28.0, 5.0, -78.0)))


class Placing(unittest.TestCase):
    def setUp(self):
        self.placer = Placer(ROWS, PLACE_NAMES)

    def test_overworld_is_exact_in_its_tile_block(self):
        p = self.placer.place("m60_42_36_00", (15.0, 0.0, 25.0))
        self.assertEqual((p["block"], p["area"], p["gx"], p["gz"]), (614236, 60, 42, 36))
        self.assertEqual((p["x"], p["z"], p["how"], p["text_id"]), (15.0, 25.0, "exact", 1000))

    def test_tile_without_rows_borrows_the_nearest_prefix(self):
        self.assertEqual(self.placer.place("m60_44_36_00", (0.0, 0.0, 0.0))["block"], 614436)

    def test_cave_item_sits_on_the_entrance_not_the_boss(self):
        p = self.placer.place("m31_04_00_00", (99.0, 99.0, 99.0))
        self.assertEqual((p["block"], p["area"], p["gx"], p["gz"]), (3104, 60, 43, 37))
        self.assertEqual((p["x"], p["y"], p["z"], p["how"], p["text_id"]), (5.0, 6.0, 7.0, "entrance", 3000))

    def test_other_dungeon_is_exact_in_its_own_map(self):
        p = self.placer.place("m12_01_00_00", (1.0, 2.0, 3.0))
        self.assertEqual((p["block"], p["area"], p["gx"], p["gz"]), (1201, 12, 1, 0))
        self.assertEqual((p["how"], p["disp_mask"]), ("exact-dungeon", 2))   # underground layer

    def test_map_without_anything_goes_through_the_map_converting_into_it(self):
        # m12_09 has no icons, grace or conversion; m12_01's point (0, 0, 0) is its (100, 0, 0).
        placer = Placer(ROWS, PLACE_NAMES, conv={"m12_01": [((0.0, 0.0, 0.0), "m12_09_00_00", (100.0, 0.0, 0.0))]})
        p = placer.place("m12_09_00_00", (110.0, 2.0, 3.0))
        self.assertEqual((p["block"], p["area"], p["gx"], p["x"], p["y"], p["z"]), (1201, 12, 1, 10.0, 2.0, 3.0))

    def test_dungeon_without_icons_goes_with_its_grace(self):
        # A Convergence dungeon under the DLC map with no map icons of its own (Raya Crystalia).
        placer = Placer(ROWS, PLACE_NAMES, graces={(31, 83): {"disp_mask": 4, "text_id": 4000}})
        p = placer.place("m31_83_00_00", (1.0, 2.0, 3.0))
        self.assertEqual((p["block"], p["how"], p["disp_mask"], p["text_id"]), (3183, "exact-dungeon", 4, 4000))


if __name__ == "__main__":
    unittest.main()
