//! Our icons, made from Convergence's own art when the game starts (iconsource.rs, iconart.rs), so
//! neither this repo nor a release carries any of it. They're cached in ClassMarkers/mod/icons/
//! and only remade when Convergence's menu files, colors.txt or an override changed.
//!
//! Anyone can replace one: a PNG at ClassMarkers/icons/<set>/<file> is used instead of
//! mod/icons/<set>/<file> (icon_path). The sets and files:
//!   notes/<Class> (path|weapons).png  in item descriptions and notes (text-sized, on the baseline)
//!   map/<Class> (path|weapons).png    the map markers (outline and class-coloured glow)
//!   map/<Class> (<Tier> rune).png     the spell rune markers, one per rune tier
//!
//! The runes' art is Oodle-compressed; Oodle is the game's own DLL, loaded here if the game hasn't
//! yet. That's safe because this runs outside the loader lock (lib.rs prepare_files).

use crate::generated::{NOTES, RUNE_ART};
use crate::iconart::{map_note_icon, map_rune_icon, text_icon};
use crate::iconsource::{atlas_icons, detail_images, oodle, DETAIL_IMAGES};
use crate::log;
use crate::noteicons::{LAYOUTS, TEXTURES};
use crate::pixels::{encode_png, Rgba};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Glow colour for a class without a colour (colors.rs): warm parchment.
const DEFAULT_GLOW: [u8; 3] = [0xE0, 0xC8, 0x9A];

/// Where an icon is: the override in icons/ if there is one, else our generated copy.
pub fn icon_path(dll_dir: &Path, set: &str, file: &str) -> PathBuf {
    let own = dll_dir.join("icons").join(set).join(file);
    if own.exists() {
        own
    } else {
        dll_dir.join("mod").join("icons").join(set).join(file)
    }
}

pub fn file_name(class: &str, which: &str) -> String {
    format!("{} ({}).png", class, which)
}

/// Make the icons that are missing or out of date.
pub fn prepare(dll_dir: &Path) {
    let hi = dll_dir.join("..").join("mod").join("menu").join("hi");
    let cache = dll_dir.join("mod").join("icons");
    let base = stamp_base(dll_dir, &hi);
    let notes_stamp = format!("{}|notes", base);
    if read(&cache.join("notes.stamp")) != notes_stamp {
        match make_notes(dll_dir, &hi, &cache) {
            Ok(n) => {
                let _ = fs::write(cache.join("notes.stamp"), &notes_stamp);
                log!("Icons: made {} note icons from Convergence's art", n);
            }
            Err(e) => log!("Icons: couldn't make the note icons ({})", e),
        }
    }
    let runes_stamp = format!("{}|runes", base);
    if read(&cache.join("runes.stamp")) != runes_stamp {
        let Some(kraken) = oodle(true) else {
            log!("Icons: the game's Oodle isn't available; rune markers use the note icon");
            return;
        };
        match make_runes(dll_dir, &hi, &cache, kraken) {
            Ok(n) => {
                let _ = fs::write(cache.join("runes.stamp"), &runes_stamp);
                log!("Icons: made {} rune icons from Convergence's art", n);
            }
            Err(e) => log!("Icons: couldn't make the rune icons ({})", e),
        }
    }
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_default()
}

/// Everything the icons are made from: our version, Convergence's menu files, colors.txt and the
/// override icons (their colours are the glows' colours).
fn stamp_base(dll_dir: &Path, hi: &Path) -> String {
    let mut s = format!("v{}", env!("CARGO_PKG_VERSION"));
    for f in [TEXTURES, LAYOUTS, DETAIL_IMAGES] {
        if let Ok(m) = fs::metadata(hi.join(f)) {
            s += &format!("|{}:{}:{:?}", f, m.len(), m.modified().ok());
        }
    }
    s += &format!("|colors:{:?}", fs::read(dll_dir.join("icons").join("colors.txt")).ok());
    if let Ok(dir) = fs::read_dir(dll_dir.join("icons").join("notes")) {
        let mut own: Vec<String> = dir
            .flatten()
            .filter_map(|e| {
                Some(format!(
                    "{}:{}:{:?}",
                    e.file_name().to_string_lossy(),
                    e.metadata().ok()?.len(),
                    e.metadata().ok()?.modified().ok()
                ))
            })
            .collect();
        own.sort();
        s += &format!("|own:{}", own.join(","));
    }
    s
}

fn write(path: PathBuf, img: &Rgba) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, encode_png(img)?).map_err(|e| format!("{} ({})", path.display(), e))
}

fn glow_of(colours: &HashMap<&'static str, String>, class: &str) -> [u8; 3] {
    colours
        .get(class)
        .and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok())
        .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
        .unwrap_or(DEFAULT_GLOW)
}

/// The notes' text icons, then (their colours known) their map icons.
fn make_notes(dll_dir: &Path, hi: &Path, cache: &Path) -> Result<usize, String> {
    let ids: Vec<u16> = NOTES.iter().map(|n| n.icon).collect();
    let art = atlas_icons(hi, &ids)?;
    let mut n = 0;
    for note in NOTES {
        if let Some(img) = art.get(&note.icon) {
            write(cache.join("notes").join(file_name(note.class, note.which)), &text_icon(img))?;
            n += 1;
        }
    }
    let colours = crate::colors::class_colors(dll_dir);
    for note in NOTES {
        if let Some(img) = art.get(&note.icon) {
            let glow = glow_of(colours, note.class);
            write(cache.join("map").join(file_name(note.class, note.which)), &map_note_icon(img, glow))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Every rune tier's map icon.
fn make_runes(
    dll_dir: &Path,
    hi: &Path,
    cache: &Path,
    kraken: crate::formats::OodleDecompress,
) -> Result<usize, String> {
    let ids: Vec<u16> = RUNE_ART.iter().map(|&(_, _, icon)| icon).collect();
    let art = detail_images(hi, &ids, kraken)?;
    let colours = crate::colors::class_colors(dll_dir);
    let mut n = 0;
    for &(class, tier, icon) in RUNE_ART {
        if let Some(img) = art.get(&icon) {
            let file = file_name(class, &format!("{} rune", tier));
            write(cache.join("map").join(file), &map_rune_icon(img, glow_of(colours, class)))?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    /// Makes every icon from real files: CM_HI=<Convergence's mod\menu\hi> CM_OUT=<ClassMarkers-like
    /// folder> CM_OODLE=<oo2core_6_win64.dll>.
    #[test]
    #[ignore = "needs real game files, see the CM_* variables above"]
    fn make_all() {
        let (Ok(hi), Ok(out), Ok(dll)) = (std::env::var("CM_HI"), std::env::var("CM_OUT"), std::env::var("CM_OODLE"))
        else {
            return;
        };
        let module = unsafe {
            windows_sys::Win32::System::LibraryLoader::LoadLibraryA(
                std::ffi::CString::new(dll).unwrap().as_ptr() as *const u8
            )
        };
        assert!(!module.is_null(), "can't load Oodle");
        let (hi, out) = (std::path::Path::new(&hi), std::path::Path::new(&out));
        let cache = out.join("mod").join("icons");
        let t = std::time::Instant::now();
        println!("notes: {:?} in {:.1} s", super::make_notes(out, hi, &cache), t.elapsed().as_secs_f32());
        let t = std::time::Instant::now();
        let kraken = crate::iconsource::oodle(false).unwrap();
        println!("runes: {:?} in {:.1} s", super::make_runes(out, hi, &cache, kraken), t.elapsed().as_secs_f32());
    }
}
