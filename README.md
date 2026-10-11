# Class Markers for The Convergence

Adds world-map markers for the items listed in The Convergence's class notes
("Note: Path of the …" / "Note: Weapons for …"): spell runes, armor and weapons, labelled with the
item name and its location. Item descriptions start with a line naming the classes whose notes
list the item (or a rune that teaches it) or that start with it, each with its note's icon and in
a colour taken from it. A note's markers show while that note is in your inventory (put a note in
the storage chest to stop tracking that class, take it out to track it again); a marker hides
once you've picked its item up there, or have it in your inventory or storage chest. Each marker
shows its class note's icon with an outline and a glow in the class colour, and its text names
the item and repeats where the note says to find it, after the note's icon.
The class notes themselves become trackers: every note is rewritten in one layout (class and
progress, its intro, then its runes / armor / weapons with counts), entries in the class colour,
turning grey once found.
A DLL loaded by me3 — Convergence's files and the game files are not modified (apart from one
fenced entry added to Convergence's launch profiles).

## Icons

All icons are plain PNGs in `icons/`, read by the plugin at every start: edit or replace them and
restart the game.

| Folder | What | Starting point |
|---|---|---|
| `icons/map/` | Map markers: `<Class> (path).png` / `(weapons).png`, shown exactly as they are (any square PNG, drawn ~84 px) | `python tools/note_icons.py` exports Convergence's note icons, tail included, the paper at the centre, with a thin uneven black outline and a glow in the class colour baked in |
| `icons/map/` | Spell rune markers, one per tier: `<Class> (<Tier> rune).png`, e.g. `Stormcaller (Shining rune).png` (falls back to the path note's icon) | `python tools/note_icons.py` exports each rune's detail image, with the same outline and glow |
| `icons/notes/` | Next to the class names in item descriptions: `<Class> (path).png` / `(weapons).png`; each class name takes the most vivid colour of its icon, or its line in `icons/colors.txt` | `python tools/note_icons.py` exports Convergence's note icons, cut to sit on the baseline |

Class colours (description text and map glow) come from the class's description icon; a line in
`icons/colors.txt` (`Class = #RRGGBB`) overrides one. A missing file falls back to the map's stock
icon or the note's own icon. Note icons can be any
size and shape: 96 image pixels are drawn as 32 screen pixels. The description box puts the
image's bottom on the text baseline and grows the line upward for anything taller, so the exported
icons are cut like a letter: no tail, no margins, two thirds of a 96 px square, resting on its
bottom edge. Changing note icons costs a few seconds once at the next start.

## Layout

| Path | What |
|---|---|
| `plugin/` | Rust DLL. Grows `WorldMapPointParam` and the PlaceName text in memory at startup. |
| `plugin/src/generated.rs` | Marker data, written by `tools/generate.py`. Don't edit by hand. |
| `tools/generate.py` | Reads Convergence's files and writes `generated.rs`. |
| `icons/` | Our icons, read by the plugin at every start (see Icons above). |
| `tools/note_icons.py` | Exports Convergence's note icons into `icons/notes/` and `icons/map/` (keeps existing files; `--force` overwrites). |
| `tools/profiles.ps1` | Used by `Install.bat` / `Uninstall.bat`. |
| `tools/update.ps1` | Used by `Update.bat`: generate, icons, build, profiles. |
| `icons/exported.json` | Hashes of the icons `note_icons.py` wrote, to tell edited icons apart. |
| `mod/` | Our me3 package (generated, not in git): the world map movie and menu icon textures with our icons. |
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

After a Convergence update: close the game and run `Update.bat`. It re-reads Convergence's files,
refreshes the icons nobody edited (edited ones are kept, see `icons/exported.json`), rebuilds the
plugin and puts it back in the launch profiles (an update replaces them).

## How it works (short)

- Item positions come from Convergence's own data: item lots → map-file treasure positions, or
  the boss/NPC whose defeat awards the lot (event scripts). Items in caves/catacombs/tunnels/gaols
  are placed on the dungeon's entrance icon. When nothing gives an exact spot: an NPC the note names that has a map
  marker (quests: "Given by Gowry" goes on Gowry), else the map the item's lot belongs to (its id,
  or the one map script that awards it), on a boss marker there the note names, the dungeon's
  entrance, a landmark or the tile's middle.
- The world map only draws a `WorldMapPointParam` row if its **row id encodes its map block**
  (`AABBnn` for dungeon map mAA_BB, `PPGGZZnn` for world tile GG/ZZ). The plugin picks a free `nn`.
- Labels are added to the PlaceName text (ids from 1,850,000,000) before any marker uses them.
- Icons: the map draws icon N as frame N of sprite 171 in `menu/02_120_worldmap.gfx`. At every
  start, before the game loads its menus, the plugin copies Convergence's movie, adds our bitmaps
  and one frame per icon (icon ids right after Convergence's last frame), and writes it to `mod/`,
  an me3 package loaded after Convergence's. A Convergence update is picked up automatically; if
  anything fails, the copy is made identical to Convergence's and markers use stock icon 88.
- Note icons: the description box draws `<img src="img://NAME">` from the menu icon atlas
  (`menu/hi/01_common.sblytbnd` = named rectangles, `01_common.tpf` = BC7 textures). The plugin
  adds rows with our icons below the texture that holds the note icons, and entries
  `MENU_ItemIcon_CM_<note id>` to its layout, in copies in `mod/`. Rebuilt only when an icon or
  Convergence's files changed (`mod/menu/hi/class_markers_icons.stamp`).
- Technique for growing params/text in memory follows
  [Map For Goblins - DLL](https://github.com/VirusAlex/ERR-MapForGoblins-DLL) by VirusAlex (MIT).

The plugin writes `ClassMarkers.log` next to the DLL on every launch.
