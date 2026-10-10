"""Match the item names written in the class notes to real items in Convergence's data.

The notes are hand-written, so names don't always equal the game's: spacing ("Long Bow" vs
"Longbow"), possessives ("Rosus Axe" vs "Rosus' Axe"), accents ("Varre" vs "Varre"), typos
("Blaphemous Blade"), word order ("Dragon Cult Crest" vs "Crest of the Dragon Cult"), several
items in one entry ("Envoy's Horn & Coded Sword"), items listed under another section
(talisman-like goods under armor), and armor sets named after a person instead of their pieces.

resolve(kind, name) -> MatchResult with the matched items and how they were found.
"""
import difflib
import glob
import json
import os
import re
import unicodedata
from collections import Counter
from dataclasses import dataclass, field

import common

# Item categories as used by ItemLotParam (lotItemCategory).
GOODS, WEAPON, ARMOR, TALISMAN, ASH = 1, 2, 3, 4, 5
TABLES = {GOODS: "GoodsName", WEAPON: "WeaponName", ARMOR: "ProtectorName",
          TALISMAN: "AccessoryName", ASH: "GemName"}
SECTION_CATEGORY = {"rune": GOODS, "weapon": WEAPON, "armor": ARMOR}
FUZZY_CUTOFF = 0.93          # only near-identical spellings count as a typo match
LEG_WORDS = ("greaves", "trousers", "leggings", "boots", "legwraps", "waistcloth")

# Note entries the rules can't settle. Keep this short and explain each one.
OVERRIDES = {}


@dataclass
class MatchResult:
    items: list = field(default_factory=list)   # [(item name, item id, category)]
    how: str = "none"


def _load(table):
    """name -> [ids] across the base and DLC layers of one FMG family (first layer wins per id)."""
    out = {}
    for f in sorted(glob.glob(os.path.join(common.ITEM_DIR, table + "*.fmg.json"))):
        for k, v in json.load(open(f, encoding="utf-8")).items():
            if v and v.strip() and not v.startswith("[ERROR]"):
                ids = out.setdefault(v.strip(), [])
                if int(k) not in ids:
                    ids.append(int(k))
    return out


NAMES = {cat: _load(t) for cat, t in TABLES.items()}


def norm(s):
    """Spelling-insensitive key: no accents, case, punctuation, spaces or possessive/plural 's."""
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode().lower()
    s = re.sub(r"'s\b|s'(?=\s|$)|'", "", s)
    words = re.findall(r"[a-z0-9]+", s)
    words = [w[:-1] if len(w) > 3 and w.endswith("s") and not w.endswith("ss") else w for w in words]
    return words


def key(s):
    return "".join(norm(s))


def word_set(s):
    return frozenset(w for w in norm(s) if w not in ("of", "the", "and"))


INDEX = {cat: {} for cat in TABLES}         # spelling key -> real names
WORDS = {cat: {} for cat in TABLES}         # word set -> real names
for _cat, _names in NAMES.items():
    for _n in _names:
        INDEX[_cat].setdefault(key(_n), []).append(_n)
        WORDS[_cat].setdefault(word_set(_n), []).append(_n)


def _items(cat, names):
    return [(n, i, cat) for n in names for i in NAMES[cat][n]]


def _find_one(name, cats):
    """One item name -> (items, how), trying each category in `cats` in order."""
    for cat in cats:
        if name in NAMES[cat]:
            return _items(cat, [name]), "exact"
    for cat in cats:
        hits = INDEX[cat].get(key(name))
        if hits:
            return _items(cat, hits), "spelling"
    for cat in cats:
        hits = WORDS[cat].get(word_set(name))
        if hits:
            return _items(cat, hits), "word order"
    words = name.split()
    if words and words[-1].lower() in LEG_WORDS and ARMOR in cats:
        for alt in LEG_WORDS:
            hits = INDEX[ARMOR].get(key(" ".join(words[:-1] + [alt])))
            if hits:
                return _items(ARMOR, hits), "leg piece name"
    for cat in cats:
        close = difflib.get_close_matches(key(name), list(INDEX[cat]), n=1, cutoff=FUZZY_CUTOFF)
        if close:
            return _items(cat, INDEX[cat][close[0]]), "typo"
    return [], "none"


def _armor_block(item_id):
    """All non-altered armor pieces sharing the 1000-id block of `item_id` (one armor set)."""
    base = item_id - item_id % 1000
    return [(n, i, ARMOR) for n, ids in NAMES[ARMOR].items() for i in ids
            if base <= i < base + 1000 and "(Altered)" not in n]


def _resolve_set(name):
    """'X Set' / 'Set of X' -> every piece of that set."""
    m = re.match(r"Set of (.+)$", name)
    phrase = m.group(1) if m else re.sub(r"\s+Set$", "", name)
    words = phrase.split()
    # Try the whole phrase, then shorter tails ("General Radahn's" -> "Radahn's"), then heads
    # ("Queen of the Full Moon" -> "Queen"); a piece matches if its name contains the phrase
    # (possessive-insensitive) as whole words.
    candidates = [" ".join(words[i:]) for i in range(len(words))] + \
                 [" ".join(words[:i]) for i in range(len(words) - 1, 0, -1)]
    def contains_words(n, cand):
        """How well a piece name fits `cand`: "Set of X" pieces are named "... of X" (so "Set of
        Night" isn't the Night Maiden set); otherwise the name contains `cand` as whole words,
        exact spelling first (so "Radahn's" finds Radahn's own set, not the Radahn Soldier set),
        then spelling-insensitive."""
        if m and n.lower().endswith(" of " + cand.lower()):
            return 4
        if re.match(re.escape(cand.lower()) + r"(?![\w'])", n.lower()):
            return 3   # "Perfumer Set" -> "Perfumer Hood", not "Depraved Perfumer Robe"
        if re.search(r"(?<![\w'])" + re.escape(cand.lower()) + r"(?![\w'])", n.lower()):
            return 2
        ck, nw = norm(cand), norm(n)
        return 1 if any(nw[i:i + len(ck)] == ck for i in range(len(nw) - len(ck) + 1)) else 0

    for cand in candidates:
        ck = norm(cand)
        if not ck or ck in (["the"], ["of"], ["house"], ["general"]):
            continue
        scored = [(contains_words(n, cand), ids) for n, ids in NAMES[ARMOR].items() if "(Altered)" not in n]
        best = max((s for s, _ in scored), default=0)
        hits = [i for s, ids in scored if s == best and best > 0 for i in ids]
        if hits:
            # Keep the id block with the most name matches: the set itself, not a look-alike
            # item from elsewhere ("Divine Beast Head" next to the Divine Beast set).
            per_block = Counter(i - i % 1000 for i in hits)
            top = max(per_block.values())
            pieces = {}
            for i in hits:
                if per_block[i - i % 1000] != top:
                    continue
                for p in _armor_block(i):
                    pieces[p[1]] = p
            how = "set" if cand == phrase else f"set via '{cand}'"
            return sorted(pieces.values(), key=lambda p: p[1]), how
    return [], "none"


def _resolve_from_siblings(name, siblings, cats):
    """An armor piece the note names wrongly ("High Page Garb") inside a group listing the rest of
    its set piece by piece: the set is the siblings' armor id block, and the piece is the one in
    that block no sibling has claimed ("High Page Clothes")."""
    claimed, blocks = set(), set()
    for sib in siblings:
        found, _ = _find_one(sib, [ARMOR])
        for _, i, _ in found:
            claimed.add(i)
            blocks.add(i - i % 1000)
    if len(blocks) != 1:
        return []
    left = [p for p in _armor_block(blocks.pop() + 0) if p[1] not in claimed]
    return left if len(left) == 1 else []


def resolve(kind, name, siblings=()):
    """`siblings`: the other entries listed in the same armor group of the note, if any."""
    section = SECTION_CATEGORY[kind]
    # Notes sometimes list an item under another section (e.g. hearts and masks that are goods).
    cats = [section] + [c for c in TABLES if c != section]

    if name in OVERRIDES:
        items = []
        for n in OVERRIDES[name]:
            items += _find_one(n, cats)[0]
        return MatchResult(items, "override")

    items, how = _find_one(name, cats)
    if items:
        return MatchResult(items, how)

    if kind == "armor" and (name.endswith(" Set") or name.startswith("Set of ")):
        items, how = _resolve_set(name)
        if items:
            return MatchResult(items, how)

    if kind == "armor" and siblings:
        items = _resolve_from_siblings(name, siblings, cats)
        if items:
            return MatchResult(items, "rest of its set")

    # Several items in one entry: "A & B", "A / B", "A and B". A part that doesn't match on its
    # own inherits the first part's leading words ("Gargoyle's Greatsword/Halberd").
    parts = [p.strip() for p in re.split(r"\s*(?:&|/|\band\b)\s*", name) if p.strip()]
    if len(parts) > 1:
        items, hows = [], []
        lead = parts[0].split()[:-1]
        for n, p in enumerate(parts):
            found, h = [], "none"
            if n > 0 and lead:   # "Halberd" in "Gargoyle's Greatsword/Halberd" = Gargoyle's Halberd
                found, h = _find_one(" ".join(lead + [p]), cats)
            if not found:
                found, h = _find_one(p, cats)
            items += found
            hows.append(h)
        if items and "none" not in hows:
            return MatchResult(items, "split: " + ", ".join(hows))
        # Fall back to the whole name read as one ("Blue and White Wooden Shield").
        whole, h = _find_one(name.replace(" and ", " "), cats)
        if whole:
            return MatchResult(whole, h)
        return MatchResult(items, "partial split" if items else "none")
    return MatchResult([], "none")
