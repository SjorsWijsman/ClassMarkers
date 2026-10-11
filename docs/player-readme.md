# Class Markers for The Convergence

Map markers and progress tracking for the items in The Convergence's class notes
("Note: Path of the …" / "Note: Weapons for …").

- **Map markers** for the spell runes, armor and weapons a class note lists, with the class's icon,
  the item's name and where the note says to find it. A note's markers show while that note is in
  your inventory: put a note in the storage chest to stop tracking that class. A marker disappears
  once you have its item.
- **Class notes as trackers:** every class note shows your progress ("Nox Oracle 5 / 10 found"),
  and found entries turn grey.
- **Item descriptions** start with the classes whose notes list the item, or that start with it.

The Convergence's files and the game's files are not changed: the mod is a plugin loaded by
Convergence's launcher, plus one entry it adds to Convergence's launch profiles.

## Install

1. Unzip so that this folder is `ConvergenceER\ClassMarkers` (next to Convergence's `mod` and
   `me3` folders).
2. Run `Install.bat`.
3. Start the game with `Start_Convergence.bat` as usual (Seamless Co-op works too).

The first start after installing or changing icons takes a few seconds longer.

**After a Convergence update** run `Install.bat` again (the update replaces the launch profiles).
The markers are made for one Convergence version: if things are missing or misplaced after an
update, get the matching Class Markers release.

**Uninstall:** run `Uninstall.bat`, then delete this folder.

## Make it yours

The icons are made from The Convergence's own art when the game starts, and kept in
`mod\icons\` (`notes\` for the text, `map\` for the map markers, one per class note and per
spell rune tier). To change one, copy it to the same place under `icons\` and edit it there: a
file in `icons\` always wins. `icons\colors.txt` sets class colours (`Class name = #RRGGBB`) for
the text and the map markers' glow.

The first start (and the first after a Convergence update) takes a few seconds longer while the
icons are made. The spell rune markers may only get their own icons from the second start.

`ClassMarkers.log` (written at every start) says what was added, and why if something wasn't.
