"""Shared paths for the Class Markers generator.

Layout:  ConvergenceER\\mod                  - Convergence's files (read only)
         ClassMarkers\\tools                 - these scripts
         ClassMarkers\\tools\\cache           - everything extracted/derived (safe to delete)
The ConvergenceER folder comes from ClassMarkers\\convergence_path.txt (this machine's setting,
not in git); without it, the folder this project sits in is used.
"""
import os
import re
import winreg

TOOLS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(TOOLS)                      # ...\ClassMarkers


def convergence_dir():
    setting = os.path.join(ROOT, "convergence_path.txt")
    if os.path.exists(setting):
        path = open(setting, encoding="utf-8").read().strip()
        if path:
            return path
    return os.path.dirname(ROOT)


MOD = os.path.join(convergence_dir(), "mod")       # ...\ConvergenceER\mod
if not os.path.isdir(MOD):
    raise SystemExit(f"Convergence's mod folder not found at {MOD}. Put the ConvergenceER folder "
                     f"path in {os.path.join(ROOT, 'convergence_path.txt')}.")
CACHE = os.path.join(TOOLS, "cache")
ITEM_DIR = os.path.join(CACHE, "item")             # decoded item text (FMG -> json)
MENU_DIR = os.path.join(CACHE, "menu")             # decoded menu text
REG_DIR = os.path.join(CACHE, "reg")               # params from regulation.bin
GENERATED_RS = os.path.join(ROOT, "plugin", "src", "generated.rs")



def elden_ring_game_dir():
    """The installed ELDEN RING\\Game folder, found through Steam's library list."""
    candidates = []
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam") as k:
            steam = winreg.QueryValueEx(k, "SteamPath")[0]
        candidates.append(steam)
        vdf = os.path.join(steam, "steamapps", "libraryfolders.vdf")
        if os.path.exists(vdf):
            candidates += re.findall(r'"path"\s+"([^"]+)"', open(vdf, encoding="utf-8").read())
    except OSError:
        pass
    for lib in candidates:
        game = os.path.join(lib.replace("\\\\", "\\"), "steamapps", "common", "ELDEN RING", "Game")
        if os.path.exists(os.path.join(game, "eldenring.exe")):
            return game
    raise SystemExit("Could not find the ELDEN RING game folder through Steam.")
