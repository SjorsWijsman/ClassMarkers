"""Parse the class notes (GoodsName/GoodsCaption) into structured per-class item lists."""
import json, os, re

import common

import json,re
strip=lambda s: re.sub(r'</?font[^>]*>','',s or '')
def items(lines):
    groups=[[]]
    for l in lines:
        l=l.strip()
        if not l:
            if groups[-1]: groups.append([])
            continue
        name,loc=l.split(' - ',1)
        groups[-1].append([name.strip(),loc.strip()])
    return [g for g in groups if g]
def build_classes():
    n=json.load(open(os.path.join(common.ITEM_DIR,'GoodsName.fmg.json'),encoding='utf-8'))
    c=json.load(open(os.path.join(common.ITEM_DIR,'GoodsCaption.fmg.json'),encoding='utf-8'))
    out=[]
    for p in range(8800,8827):
        cls=n[str(p)].replace('Note: Path of the ','')
        d={'id':re.sub(r'[^a-z]+','-',cls.lower()).strip('-'),'name':cls,'pathId':p,'summary':None,'school':None,'runes':[],'armor':[],'weapons':[],'weaponsId':None}
        sec=None; buf={'runes':[],'armor':[]}
        for l in strip(c.get(str(p))).split('\n'):
            s=l.strip()
            m=re.match(r'The following locations have (.+) spell runes:',s)
            if m: d['school']=m.group(1); sec='runes'; continue
            if s.startswith('The notable armor'): sec='armor'; continue
            if ' - ' not in s and s:
                d['summary']=s; continue
            buf[sec or 'armor'].append(l)
        d['runes']=[x for g in items(buf['runes']) for x in g]
        d['armor']=items(buf['armor'])
        w=str(p+70)
        if 'Weapons for' in n.get(w,''):
            d['weaponsId']=int(w); wl=[]
            for l in strip(c.get(w)).split('\n'):
                s=l.strip()
                if s.startswith('The notable weapons'): continue
                if s and ' - ' not in s:
                    d['wsummary']=s; continue
                wl.append(l)
            d['weapons']=[x for g in items(wl) for x in g]
        out.append(d)
    json.dump(out,open(os.path.join(common.CACHE,'classes.json'),'w',encoding='utf-8'),ensure_ascii=False,indent=1)
    return out
