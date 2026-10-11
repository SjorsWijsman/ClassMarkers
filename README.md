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

The plugin makes every icon itself from Convergence's own art when the game starts
(`plugin/src/iconcache.rs`: the class notes from the menu atlas, the runes from their detail
images), so this repo and the releases contain none of it. They're cached in `mod/icons/` and
remade only when Convergence's menu files, `icons/colors.txt` or an override change (a few seconds).

| File (in `mod/icons/`, overridable in `icons/`) | What |
|---|---|
| `notes/<Class> (path).png` / `(weapons).png` | Next to the class names in item descriptions and notes: the note without its tail, two thirds of a 96 px square, on its bottom edge (a text box puts an image's bottom on the baseline; 96 image px = 32 screen px) |
| `map/<Class> (path).png` / `(weapons).png` | Map markers: the whole note, its paper at the centre, a thin black outline and a glow in the class colour (any square PNG, drawn ~84 px) |
| `map/<Class> (<Tier> rune).png` | Spell rune markers, one per tier, from the rune's detail image, same outline and glow |

To change one, copy it from `mod/icons/` to the same place under `icons/` and edit it there: a
file in `icons/` always wins. Class colours (description text and map glow) are the most vivid
colour of the class's `notes/` icon; a line in `icons/colors.txt` (`Class = #RRGGBB`) overrides
one. The rune markers' icons are made with the game's Oodle; if it isn't loaded yet when the
plugin starts, they're made on a thread and used from the next start.

## Layout

| Path | What |
|---|---|
| `plugin/` | Rust DLL. Grows `WorldMapPointParam` and the PlaceName text in memory at startup. |
| `plugin/src/generated.rs` | Marker data, written by `tools/generate.py`. Don't edit by hand. |
| `tools/generate.py` | Reads Convergence's files and writes `generated.rs`. |
| `icons/` | Overrides for the generated icons, and `colors.txt` (see Icons above). |
| `tools/profiles.ps1` | Used by `Install.bat` / `Uninstall.bat`. |
| `tools/update.ps1` | Used by `Update.bat`: generate, build, profiles. |
| `mod/` | Made by the plugin (not in git): our me3 package (world map movie and menu icon textures with our icons) and the icon cache (`mod/icons/`). |
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
rebuilds the plugin and puts it back in the launch profiles (an update replaces them). The icons
remake themselves at the next start.

## License and credits

The code is MIT (see `LICENSE`). Nothing of ELDEN RING's or The Convergence's art or text is in
this repo or the releases: the plugin reads the class notes, item names and art from the game at
startup. `plugin/src/generated.rs` holds only numbers derived from Convergence's data (ids, flags,
positions, which note line an entry is) and the class names. Release zips list the Rust crates in
the DLL in `THIRD-PARTY-NOTICES.md`.

- The technique for growing game tables and text in memory follows
  [Map For Goblins - DLL](https://github.com/VirusAlex/ERR-MapForGoblins-DLL) by VirusAlex (MIT).
- Game structures: [fromsoftware-rs](https://github.com/vswarte/fromsoftware-rs) (MIT/Apache-2.0).
- The Convergence by The Convergence team; ELDEN RING by FromSoftware.

## Releases

`tools/package.ps1` packs `dist/ClassMarkers-<version>.zip` for players: the DLL, `icons/`,
`Install.bat`, `Uninstall.bat`, `tools/profiles.ps1` and `docs/player-readme.md` as its README,
laid out to be unzipped into `ConvergenceER`. Installing from that folder needs no Python, Rust or
`convergence_path.txt`.

GitHub builds releases by itself (`.github/workflows/release.yml`): bump `version` in
`plugin/Cargo.toml`, commit, then push a matching tag:

    git tag v1.0.0
    git push origin v1.0.0

The workflow checks the tag matches the version, builds the plugin, packs the zip and attaches it
to a new release with notes from the commits.

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
