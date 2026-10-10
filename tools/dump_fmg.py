import ctypes, struct, sys, os, json

import common

oo = ctypes.WinDLL(os.path.join(common.elden_ring_game_dir(), "oo2core_6_win64.dll"))
oo.OodleLZ_Decompress.restype = ctypes.c_int64
oo.OodleLZ_Decompress.argtypes = [ctypes.c_char_p, ctypes.c_int64, ctypes.c_char_p, ctypes.c_int64,
                                  ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_int64,
                                  ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int64, ctypes.c_int]

def dcx(data):
    assert data[:4] == b"DCX\0"
    usize, csize = struct.unpack(">II", data[0x1C:0x24])
    fmt = data[0x28:0x2C]
    comp = data[0x4C:0x4C + csize]
    if fmt == b"KRAK":
        out = ctypes.create_string_buffer(usize)
        n = oo.OodleLZ_Decompress(comp, csize, out, usize, 0, 0, 0, None, 0, None, None, None, 0, 3)
        assert n == usize, n
        return out.raw
    if fmt == b"ZSTD":
        import zstandard
        return zstandard.ZstdDecompressor().decompress(comp, max_output_size=usize)
    import zlib
    return zlib.decompress(comp)

def bnd4(data):
    assert data[:4] == b"BND4"
    count = struct.unpack_from("<i", data, 0x0C)[0]
    hsize = struct.unpack_from("<q", data, 0x20)[0]
    unicode = data[0x30]
    fmt = int(f"{data[0x31]:08b}"[::-1], 2)
    files = []
    for i in range(count):
        o = 0x40 + i * hsize
        csize, = struct.unpack_from("<q", data, o + 8)
        p = o + 16
        if fmt & 0x20:  # compression -> uncompressed size field
            p += 8
        if fmt & 0x10:  # long offsets
            off, = struct.unpack_from("<q", data, p); p += 8
        else:
            off, = struct.unpack_from("<I", data, p); p += 4
        fid = None
        if fmt & 0x02:
            fid, = struct.unpack_from("<i", data, p); p += 4
        name = None
        if fmt & 0x0C:
            no, = struct.unpack_from("<I", data, p); p += 4
            if unicode:
                e = no
                while data[e:e+2] != b"\0\0": e += 2
                name = data[no:e].decode("utf-16-le")
            else:
                name = data[no:data.index(b"\0", no)].decode("shift_jis")
        files.append((fid, name, data[off:off + csize]))
    return files

def fmg(data):
    ver = data[2]
    assert ver == 2, ver
    groups, nstr = struct.unpack_from("<ii", data, 0x0C)
    stroff, = struct.unpack_from("<q", data, 0x18)
    out = {}
    for g in range(groups):
        idx, first, last = struct.unpack_from("<iii", data, 0x28 + g * 16)
        for k in range(last - first + 1):
            so, = struct.unpack_from("<q", data, stroff + (idx + k) * 8)
            if so == 0:
                continue
            e = so
            while data[e:e+2] != b"\0\0": e += 2
            out[first + k] = data[so:e].decode("utf-16-le")
    return out

def extract_msgbnd(src, dst):
    """Decode every FMG in a .msgbnd.dcx to <dst>/<name>.fmg.json."""
    os.makedirs(dst, exist_ok=True)
    for fid, name, blob in bnd4(dcx(open(src, "rb").read())):
        base = os.path.basename(name.replace("\\", "/"))
        with open(os.path.join(dst, base + ".json"), "w", encoding="utf-8") as f:
            json.dump(fmg(blob), f, ensure_ascii=False, indent=1)
