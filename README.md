# Class Markers for The Convergence

Adds world-map markers for the items listed in The Convergence's class notes
("Note: Path of the …" / "Note: Weapons for …"): spell runes, armor and weapons, labelled with the
item name and its location. Pure DLL loaded by me3 — Convergence's files and the game files are
not modified (apart from one entry added to Convergence's launch profiles).

## Layout

| Path | What |
|---|---|
| `plugin/` | Rust DLL. Grows `WorldMapPointParam` and the PlaceName text in memory at startup. |
| `plugin/src/generated.rs` | Marker data, written by `tools/generate.py`. Don't edit by hand. |
| `tools/generate.py` | Reads Convergence's files and writes `generated.rs`. |
| `tools/profiles.ps1` | Used by `Install.bat` / `Uninstall.bat`. |
| `available_map_icons.png` | Every icon the map can show, with its icon id. |

## Setup on a new machine

1. Clone anywhere (e.g. `D:\Code\Elden Ring Mods\ClassMarkers`).
2. Create `convergence_path.txt` in the project root containing the ConvergenceER folder path, e.g.
   `D:\Games\Elden Ring The Convergence\ConvergenceER`.
3. Python 3.10+ with `cryptography`, then: `pip install --target tools/pylib zstandard`.
4. `cd tools && python generate.py`, then `cd ../plugin && cargo build --release` and copy
   `plugin/target/release/ClassMarkers.dll` to the project root.
5. Run `Install.bat`. It adds the plugin to Convergence's `.me3` profiles and links
   `ConvergenceER\ClassMarkers` to this folder. Start the game with `Start_Convergence.bat`.

After a Convergence update: run `generate.py`, rebuild, and run `Install.bat` again (an update
replaces the profiles).

## How it works (short)

- Item positions come from Convergence's own data: item lots → map-file treasure positions, or
  the boss/NPC whose defeat awards the lot (event scripts). Items in caves/catacombs/tunnels/gaols
  are placed on the dungeon's entrance icon.
- The world map only draws a `WorldMapPointParam` row if its **row id encodes its map block**
  (`AABBnn` for dungeon map mAA_BB, `PPGGZZnn` for world tile GG/ZZ). The plugin picks a free `nn`.
- Labels are added to the PlaceName text (ids from 1,850,000,000) before any marker uses them.
- Technique for growing params/text in memory follows
  [Map For Goblins - DLL](https://github.com/VirusAlex/ERR-MapForGoblins-DLL) by VirusAlex (MIT).

The plugin writes `ClassMarkers.log` next to the DLL on every launch.
