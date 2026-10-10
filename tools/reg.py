"""Decrypt regulation.bin and write each param to <dst>/<Name>.param."""
import os

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

from dump_fmg import bnd4, dcx

# Elden Ring's regulation.bin AES-256-CBC key (public; same as SoulsFormats' SFUtil).
KEY = bytes.fromhex("99BFFC366A6BC8C6F5827D093602D676C42892A01C207FB024D3AF4E493FEF99")


def extract_regulation(src, dst):
    raw = open(src, "rb").read()
    d = Cipher(algorithms.AES(KEY), modes.CBC(raw[:16])).decryptor()
    data = dcx(d.update(raw[16:]) + d.finalize())
    os.makedirs(dst, exist_ok=True)
    n = 0
    for _, name, blob in bnd4(data):
        open(os.path.join(dst, os.path.basename(name.replace("\\", "/"))), "wb").write(blob)
        n += 1
    return n
