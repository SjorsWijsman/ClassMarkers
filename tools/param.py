"""Read a decrypted .param file (see reg.py): row id -> row bytes, and the row size."""
import struct


def rows(p):
    d = open(p, 'rb').read()
    n = struct.unpack_from('<H', d, 0x0A)[0]
    # Row locators: id, (unused), data offset, name offset. Rows are packed back to back.
    ents = [struct.unpack_from('<iiqq', d, 0x40 + i * 24) for i in range(n)]
    size = ents[1][2] - ents[0][2]
    return {rid: d[off:off + size] for rid, _, off, _ in ents}, size
