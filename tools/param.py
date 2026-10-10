import struct
def rows(p):
    d=open(p,'rb').read()
    n=struct.unpack_from('<H',d,0x0A)[0]
    ents=[struct.unpack_from('<iiqq',d,0x40+i*24) for i in range(n)]
    out={}
    for i,(rid,_,off,_) in enumerate(ents):
        end=ents[i+1][2] if i+1<n else None
        out[rid]=(off,end)
    size=ents[1][2]-ents[0][2]
    return {rid:d[o:o+size] for rid,(o,e) in out.items()}, size
