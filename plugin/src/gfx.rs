//! Our own map icons.
//!
//! The world map draws icon N as frame N of one Flash sprite (character 171) in Convergence's
//! menu/02_120_worldmap.gfx. To add icons we write a copy of that file with our bitmaps
//! (DefineBitsLossless2) and one extra frame per icon into our own me3 package
//! (ClassMarkers/mod/menu/), which the profile loads after Convergence's, so the game reads ours.
//! Convergence's file is never touched.
//!
//! The copy is rebuilt from Convergence's current file at every start, before the game loads any
//! menu, so a Convergence update is picked up by itself. me3 only serves package files that
//! existed when the game launched, so Install.bat puts a placeholder there. When anything goes
//! wrong the copy is made identical to Convergence's file and the markers use a stock icon.
//!
//! Each marker shows its class note's icon, map/<Class> (path|weapons).png, and a spell rune marker
//! the icon of its rune's tier, map/<Class> (<Tier> rune).png: made from Convergence's art with
//! outline and glow, or your own from ClassMarkers/icons/ (see iconcache.rs; any square PNG works).
//! A rune marker without a tier icon uses its note's icon; a note without one uses the stock icon.

use crate::formats::u16_at;
use crate::generated::NOTES;
use crate::log;
use crate::pixels::{decode_png, Rgba};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// File name inside menu/, in Convergence's package and in ours.
const MOVIE: &str = "02_120_worldmap.gfx";
/// The sprite whose frames are the map icons.
const ICON_SPRITE: u16 = 171;
/// On-map size of our icon images in pixels; the note itself takes about half (the rest is outline
/// and glow), next to the stock icons' 70-78.
const ICON_PX: f64 = 84.0;

/// A DefineBitsLossless2 body without the character id: format 5 (32-bit), width, height, then
/// zlib'd rows of premultiplied [A, R, G, B].
fn lossless2_body(img: &Rgba) -> Vec<u8> {
    let argb: Vec<u8> = img
        .data
        .chunks(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            [p[3], (p[0] as u32 * a / 255) as u8, (p[1] as u32 * a / 255) as u8, (p[2] as u32 * a / 255) as u8]
        })
        .collect();
    let mut body = vec![5];
    body.extend_from_slice(&(img.width as u16).to_le_bytes());
    body.extend_from_slice(&(img.height as u16).to_le_bytes());
    body.extend(miniz_oxide::deflate::compress_to_vec_zlib(&argb, 9));
    body
}

/// Where a note's map icon file is.
pub fn note_icon_path(dll_dir: &Path, class: &str, which: &str) -> std::path::PathBuf {
    crate::iconcache::icon_path(dll_dir, "map", &crate::iconcache::file_name(class, which))
}

/// Spell rune tiers, the first word of a rune's name ("Shimmering Rune of the Storm").
pub const RUNE_TIERS: [&str; 6] = ["Faint", "Shimmering", "Glowing", "Shining", "Radiant", "Shadow"];

/// Our map icons: (note goods id, rune tier or "" for the note's own icon) -> icon id.
pub type MapIcons = HashMap<(u32, &'static str), u16>;

/// Rebuild our copy of the world map movie. `dll_dir` = ConvergenceER\ClassMarkers.
/// Returns the icons that were added (empty = stock icons).
pub fn prepare(dll_dir: &Path) -> MapIcons {
    let ours = dll_dir.join("mod").join("menu").join(MOVIE);
    let theirs = dll_dir.join("..").join("mod").join("menu").join(MOVIE);
    if !ours.exists() {
        log!("Icons: {} is missing (run Install.bat); using a stock icon", ours.display());
        return MapIcons::new();
    }
    let src = match fs::read(&theirs) {
        Ok(s) => s,
        Err(e) => {
            log!("Icons: can't read {} ({}); using a stock icon", theirs.display(), e);
            return MapIcons::new();
        }
    };

    let mut icons = Vec::new();
    let mut keys = Vec::new(); // (key, index in `icons`)
    for note in NOTES {
        // The note's icon, and (spell runes are listed in the "path" note) one per rune tier.
        let tiers: &[&'static str] = if note.which == "path" { &RUNE_TIERS } else { &[] };
        for tier in std::iter::once("").chain(tiers.iter().copied()) {
            let file = if tier.is_empty() { note.which.to_string() } else { format!("{} rune", tier) };
            let path = note_icon_path(dll_dir, note.class, &file);
            let Ok(bytes) = fs::read(&path) else { continue };
            match decode_png(&bytes) {
                Ok(img) => {
                    keys.push(((note.goods_id, tier), icons.len()));
                    icons.push(lossless2_body(&img));
                }
                Err(e) => log!("Icons: can't read {} ({}); using another icon there", path.display(), e),
            }
        }
    }

    let built = if icons.is_empty() { Err("no map icon files".to_string()) } else { build(&src, &icons) };
    let (out, first) = match built {
        Ok(r) => r,
        Err(e) => {
            log!("Icons: can't add them to the world map ({}); using a stock icon", e);
            let _ = fs::write(&ours, &src); // never leave a stale or broken copy in the way
            return MapIcons::new();
        }
    };
    if fs::read(&ours).ok().as_deref() != Some(&out[..]) {
        if let Err(e) = fs::write(&ours, &out) {
            log!("Icons: can't write {} ({}); using a stock icon", ours.display(), e);
            return MapIcons::new();
        }
    }
    let runes = keys.iter().filter(|((_, tier), _)| !tier.is_empty()).count();
    log!(
        "Icons: {} note and {} rune icons added as icon ids {}-{}",
        icons.len() - runes,
        runes,
        first,
        first + icons.len() as u16 - 1
    );
    keys.into_iter().map(|(key, i)| (key, first + i as u16)).collect()
}

struct Tag {
    code: u16,
    /// Offset of the tag header.
    start: usize,
    /// Offset of the body and its length.
    body: usize,
    len: usize,
}

impl Tag {
    fn end(&self) -> usize {
        self.body + self.len
    }
}

/// The tags in `b[pos..end]`, up to and including the End tag.
fn read_tags(b: &[u8], mut pos: usize, end: usize) -> Result<Vec<Tag>, String> {
    let mut out = Vec::new();
    while pos + 2 <= end {
        let start = pos;
        let h = u16_at(b, pos);
        pos += 2;
        let mut len = (h & 63) as usize;
        if len == 63 {
            len = u32::from_le_bytes(b.get(pos..pos + 4).ok_or("truncated tag")?.try_into().unwrap()) as usize;
            pos += 4;
        }
        if pos + len > end {
            return Err("tag runs past the end".into());
        }
        out.push(Tag { code: h >> 6, start, body: pos, len });
        pos += len;
        if h >> 6 == 0 {
            break;
        }
    }
    Ok(out)
}

/// Tags whose body starts with the character id they define.
fn defines_character(code: u16) -> bool {
    matches!(
        code,
        2 | 6
            | 10
            | 11
            | 20
            | 21
            | 22
            | 32
            | 33
            | 34
            | 35
            | 36
            | 37
            | 39
            | 46
            | 48
            | 60
            | 75
            | 83
            | 84
            | 87
            | 90
            | 91
            | 1009
    )
}

fn tag(code: u16, body: &[u8], long: bool) -> Vec<u8> {
    let mut t = Vec::with_capacity(body.len() + 6);
    if body.len() < 63 && !long {
        t.extend_from_slice(&((code << 6) | body.len() as u16).to_le_bytes());
    } else {
        t.extend_from_slice(&((code << 6) | 63).to_le_bytes());
        t.extend_from_slice(&(body.len() as u32).to_le_bytes());
    }
    t.extend_from_slice(body);
    t
}

/// SWF MATRIX with a uniform scale and a translation (in twips).
fn matrix(scale: f64, tx: i32, ty: i32) -> Vec<u8> {
    fn bits_for(v: i32) -> u32 {
        // signed: enough bits for the magnitude plus a sign bit
        (32 - if v < 0 { !v } else { v }.leading_zeros()) + 1
    }
    let s = (scale * 65536.0).round() as i32;
    let mut w = BitWriter::default();
    w.put(1, 1); // HasScale
    let n = bits_for(s);
    w.put(n, 5);
    w.put(s as u32, n);
    w.put(s as u32, n);
    w.put(0, 1); // HasRotate
    let n = bits_for(tx).max(bits_for(ty));
    w.put(n, 5);
    w.put(tx as u32, n);
    w.put(ty as u32, n);
    w.finish()
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, v: u32, bits: u32) {
        for i in (0..bits).rev() {
            self.acc = (self.acc << 1) | ((v >> i) & 1);
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.n = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push((self.acc << (8 - self.n)) as u8);
        }
        self.out
    }
}

/// Convergence's movie with our icons (DefineBitsLossless2 bodies) added -> (new file, icon id of
/// our first icon).
fn build(src: &[u8], icons: &[Vec<u8>]) -> Result<(Vec<u8>, u16), String> {
    if src.len() < 12 || &src[..3] != b"GFX" {
        return Err("not an uncompressed GFX movie".into());
    }
    let rect_bits = (src[8] >> 3) as usize;
    let first_tag = 8 + (5 + 4 * rect_bits).div_ceil(8) + 4;
    let tags = read_tags(src, first_tag, src.len())?;

    // New character ids above every id the movie defines.
    let max_id =
        tags.iter().filter(|t| defines_character(t.code) && t.len >= 2).map(|t| u16_at(src, t.body)).max().unwrap_or(0);
    let first_char = max_id as u32 + 1;
    if first_char + icons.len() as u32 > u16::MAX as u32 {
        return Err("no free character ids".into());
    }

    let sprite = tags
        .iter()
        .find(|t| t.code == 39 && t.len >= 4 && u16_at(src, t.body) == ICON_SPRITE)
        .ok_or("icon sprite not found")?;
    let frames = u16_at(src, sprite.body + 2);
    let inner = read_tags(src, sprite.body + 4, sprite.end())?;

    // Which depths are occupied after the sprite's last frame: our first frame clears them.
    let mut live = std::collections::BTreeSet::new();
    for t in &inner {
        let b = &src[t.body..t.end()];
        match t.code {
            26 if b.len() >= 3 && b[0] & 0x02 != 0 => {
                live.insert(u16_at(b, 1));
            }
            70 if b.len() >= 4 && b[0] & 0x02 != 0 => {
                live.insert(u16_at(b, 2));
            }
            28 if b.len() >= 2 => {
                live.remove(&u16_at(b, 0));
            }
            5 if b.len() >= 4 => {
                live.remove(&u16_at(b, 2));
            }
            _ => {}
        }
    }

    // Our bitmaps, defined right before the sprite.
    let mut bitmaps = Vec::new();
    for (i, body) in icons.iter().enumerate() {
        let mut b = ((first_char + i as u32) as u16).to_le_bytes().to_vec();
        b.extend_from_slice(body);
        bitmaps.extend(tag(36, &b, true));
    }

    // The sprite: its own tags (without End), one frame per icon, End.
    let mut body = Vec::new();
    body.extend_from_slice(&ICON_SPRITE.to_le_bytes());
    body.extend_from_slice(&(frames + icons.len() as u16).to_le_bytes());
    let end_tag = inner.last().filter(|t| t.code == 0).ok_or("icon sprite has no End tag")?;
    body.extend_from_slice(&src[sprite.body + 4..end_tag.start]);
    for (i, bmp) in icons.iter().enumerate() {
        let size = u16_at(bmp, 1).max(u16_at(bmp, 3)) as f64;
        let depths: Vec<u16> = if i == 0 { live.iter().copied().collect() } else { vec![1] };
        for d in depths {
            body.extend(tag(28, &d.to_le_bytes(), false)); // RemoveObject2
        }
        // PlaceObject3: HasMatrix | HasCharacter, HasImage (as the stock icon frames), depth 1.
        let mut po = vec![0x06, 0x10];
        po.extend_from_slice(&1u16.to_le_bytes());
        po.extend_from_slice(&((first_char + i as u32) as u16).to_le_bytes());
        let half = -(ICON_PX / 2.0 * 20.0).round() as i32;
        po.extend(matrix(ICON_PX / size, half, half));
        body.extend(tag(70, &po, false));
        body.extend(tag(1, &[], false)); // ShowFrame
    }
    body.extend(tag(0, &[], false));

    let mut out = Vec::with_capacity(src.len() + bitmaps.len() + 256);
    out.extend_from_slice(&src[..sprite.start]);
    out.extend(bitmaps);
    out.extend(tag(39, &body, true));
    out.extend_from_slice(&src[sprite.end()..]);
    let len = out.len() as u32;
    out[4..8].copy_from_slice(&len.to_le_bytes());
    Ok((out, frames + 1))
}

#[cfg(test)]
mod tests {
    /// Builds our copy from a real movie: CM_GFX_SRC=<Convergence's file> CM_GFX_OUT=<output>.
    #[test]
    #[ignore = "needs real game files, see the CM_* variables above"]
    fn build_from_real_movie() {
        let (Ok(src), Ok(out)) = (std::env::var("CM_GFX_SRC"), std::env::var("CM_GFX_OUT")) else {
            return;
        };
        let dot = crate::pixels::Rgba { width: 8, height: 8, data: [255u8, 200, 100, 255].repeat(64) };
        let icons = vec![super::lossless2_body(&dot)];
        let (bytes, first) = super::build(&std::fs::read(src).unwrap(), &icons).unwrap();
        std::fs::write(out, bytes).unwrap();
        println!("first icon id {}", first);
    }
}
