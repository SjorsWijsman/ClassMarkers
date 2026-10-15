"""Parse the class notes (GoodsName/GoodsCaption) into structured per-class item lists."""
import json
import os
import re

import common

PATH_PREFIX = "Note: Path of the "     # the class notes' names, which find them
WEAPONS_PREFIX = "Note: Weapons for "


def strip(s):
    """Text without its <font> tags."""
    return re.sub(r'</?font[^>]*>', '', s or '')


def items(lines):
    """Note lines "<name> - <where>" -> groups of [name, where], split at blank lines."""
    groups = [[]]
    for line in lines:
        line = line.strip()
        if not line:
            if groups[-1]:
                groups.append([])
            continue
        name, loc = line.split(' - ', 1)
        groups[-1].append([name.strip(), loc.strip()])
    return [g for g in groups if g]


def build_classes():
    names = json.load(open(os.path.join(common.ITEM_DIR, 'GoodsName.fmg.json'), encoding='utf-8'))
    captions = json.load(open(os.path.join(common.ITEM_DIR, 'GoodsCaption.fmg.json'), encoding='utf-8'))
    # By name, so a class Convergence adds is picked up: path notes in id order, and each class's
    # weapons note (not every class has one).
    path_notes = sorted((int(k), v[len(PATH_PREFIX):]) for k, v in names.items() if v and v.startswith(PATH_PREFIX))
    weapons_notes = {v[len(WEAPONS_PREFIX):]: int(k) for k, v in names.items() if v and v.startswith(WEAPONS_PREFIX)}
    out = []
    for path_id, cls in path_notes:
        c = {'id': re.sub(r'[^a-z]+', '-', cls.lower()).strip('-'), 'name': cls, 'pathId': path_id,
             'summary': None, 'school': None, 'runes': [], 'armor': [], 'weapons': [], 'weaponsId': None}
        section = None
        lines = {'runes': [], 'armor': []}
        for line in strip(captions.get(str(path_id))).split('\n'):
            s = line.strip()
            school = re.match(r'The following locations have (.+) spell runes:', s)
            if school:
                c['school'] = school.group(1)
                section = 'runes'
                continue
            if s.startswith('The notable armor'):
                section = 'armor'
                continue
            if s and ' - ' not in s:
                c['summary'] = s
                continue
            lines[section or 'armor'].append(line)
        c['runes'] = [x for g in items(lines['runes']) for x in g]
        c['armor'] = items(lines['armor'])

        weapons_id = weapons_notes.get(cls)
        if weapons_id:
            c['weaponsId'] = weapons_id
            weapon_lines = []
            for line in strip(captions.get(str(weapons_id))).split('\n'):
                s = line.strip()
                if s.startswith('The notable weapons'):
                    continue
                if s and ' - ' not in s:
                    c['wsummary'] = s
                    continue
                weapon_lines.append(line)
            c['weapons'] = [x for g in items(weapon_lines) for x in g]
        out.append(c)
    json.dump(out, open(os.path.join(common.CACHE, 'classes.json'), 'w', encoding='utf-8'),
              ensure_ascii=False, indent=1)
    return out
