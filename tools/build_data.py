"""Parse the class notes (GoodsName/GoodsCaption) into structured per-class item lists."""
import json
import os
import re

import common

PATH_NOTES = range(8800, 8827)   # "Note: Path of the <class>" goods ids
WEAPONS_OFFSET = 70              # its "Note: Weapons for <class>" is the path note's id + 70


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
    out = []
    for path_id in PATH_NOTES:
        cls = names[str(path_id)].replace('Note: Path of the ', '')
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

        weapons_id = str(path_id + WEAPONS_OFFSET)
        if 'Weapons for' in names.get(weapons_id, ''):
            c['weaponsId'] = int(weapons_id)
            weapon_lines = []
            for line in strip(captions.get(weapons_id)).split('\n'):
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
