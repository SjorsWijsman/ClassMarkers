"""Note entry -> item matching (matching.py), on a small made-up item table.

Run from the repo root: python -m unittest discover -s tools/tests
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import matching  # noqa: E402
from matching import ARMOR, GOODS, TALISMAN, WEAPON, norm, resolve  # noqa: E402

NAMES = {
    GOODS: {"Shimmering Rune of Night": [8353]},
    WEAPON: {
        "Longbow": [40000000],
        "Rosus' Axe": [15040000],
        "Blasphemous Blade": [8080000],
        "Gargoyle's Greatsword": [18030000],
        "Gargoyle's Halberd": [18040000],
        "Envoy's Horn": [13040000],
        "Coded Sword": [2100000],
        "Várre's Bouquet": [6000000],
        "Blue-White Wooden Shield": [31010000],
    },
    ARMOR: {
        "Knight Greaves": [200300],
        # A set: four pieces in one id block, plus an altered piece and a look-alike in another block.
        "Perfumer Hood": [300000],
        "Perfumer Robe": [300100],
        "Perfumer Robe (Altered)": [300101],
        "Perfumer Gloves": [300200],
        "Perfumer Sarong": [300300],
        "Depraved Perfumer Robe": [310100],
        # A set the note lists piece by piece, naming one piece wrongly ("Garb").
        "High Page Hood": [400000],
        "High Page Clothes": [400100],
        "High Page Gloves": [400200],
        # The pieces of matching.OVERRIDES' "House Marais Set".
        "Marais Mask": [500000],
        "Marais Robe": [500100],
        "Bloodsoaked Manchettes": [600200],
        "Bloodsoaked Wraps": [600300],
    },
    TALISMAN: {"Crest of the Dragon Cult": [1000]},
}


def ids(result):
    return sorted(i for _, i, _ in result.items)


class Matching(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.real = dict(matching.NAMES)
        matching.use_names(NAMES)

    @classmethod
    def tearDownClass(cls):
        matching.use_names(cls.real)

    def test_norm_ignores_case_accents_possessives_and_plurals(self):
        self.assertEqual(norm("Rosus' Axe"), norm("rosus axe"))
        self.assertEqual(norm("Várre's Bouquet"), ["varre", "bouquet"])
        self.assertEqual(norm("Knight Greaves"), ["knight", "greave"])
        self.assertEqual(norm("Glass"), ["glass"])   # "ss" isn't a plural

    def test_exact(self):
        r = resolve("rune", "Shimmering Rune of Night")
        self.assertEqual((r.items, r.how), ([("Shimmering Rune of Night", 8353, GOODS)], "exact"))

    def test_spelling(self):
        for name, item in [("Long Bow", 40000000), ("Rosus Axe", 15040000), ("Varre's Bouquet", 6000000)]:
            with self.subTest(name=name):
                r = resolve("weapon", name)
                self.assertEqual((ids(r), r.how), ([item], "spelling"))

    def test_word_order_in_another_section(self):
        # Listed under armor, but it's a talisman.
        r = resolve("armor", "Dragon Cult Crest")
        self.assertEqual((r.items, r.how), ([("Crest of the Dragon Cult", 1000, TALISMAN)], "word order"))

    def test_leg_piece_name(self):
        r = resolve("armor", "Knight Trousers")
        self.assertEqual((ids(r), r.how), ([200300], "leg piece name"))

    def test_typo(self):
        r = resolve("weapon", "Blaphemous Blade")
        self.assertEqual((ids(r), r.how), ([8080000], "typo"))

    def test_set_takes_its_block_without_altered_or_look_alikes(self):
        r = resolve("armor", "Perfumer Set")
        self.assertEqual((ids(r), r.how), ([300000, 300100, 300200, 300300], "set"))

    def test_misnamed_piece_is_the_rest_of_its_set(self):
        r = resolve("armor", "High Page Garb", siblings=("High Page Hood", "High Page Gloves"))
        self.assertEqual((ids(r), r.how), ([400100], "rest of its set"))

    def test_override(self):
        r = resolve("armor", "House Marais Set")
        self.assertEqual((ids(r), r.how), ([500000, 500100, 600200, 600300], "override"))

    def test_split_entry(self):
        r = resolve("weapon", "Envoy's Horn & Coded Sword")
        self.assertEqual((ids(r), r.how), ([2100000, 13040000], "split: exact, exact"))

    def test_split_part_inherits_leading_words(self):
        r = resolve("weapon", "Gargoyle's Greatsword/Halberd")
        self.assertEqual(ids(r), [18030000, 18040000])

    def test_and_inside_a_name_is_not_a_split(self):
        # "and" is ignored in word sets, so the whole name matches before any split is tried.
        r = resolve("weapon", "Blue and White Wooden Shield")
        self.assertEqual((ids(r), r.how), ([31010000], "word order"))

    def test_unknown(self):
        r = resolve("weapon", "Sword of Nothing at All")
        self.assertEqual((r.items, r.how), ([], "none"))


if __name__ == "__main__":
    unittest.main()
