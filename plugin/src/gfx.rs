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

use crate::log;
use std::fs;
use std::path::Path;

/// File name inside menu/, in Convergence's package and in ours.
const MOVIE: &str = "02_120_worldmap.gfx";
/// The sprite whose frames are the map icons.
const ICON_SPRITE: u16 = 171;
/// On-map size of our icons in pixels (the stock round icons are 70-78).
const ICON_PX: f64 = 72.0;

/// Our icons, in the order prepare() returns their ids: DefineBitsLossless2 bodies without the
/// character id, written by tools/icons.py.
const ICONS: [(&str, &[u8]); 3] = [
    ("weapon", include_bytes!("icons/weapon.bin")),
    ("armor", include_bytes!("icons/armor.bin")),
    ("rune", include_bytes!("icons/rune.bin")),
];

/// Icon ids of our weapon, armor and rune icons.
#[derive(Clone, Copy)]
pub struct IconIds {
    pub weapon: u16,
    pub armor: u16,
    pub rune: u16,
}

/// Rebuild our copy of the world map movie. `dll_dir` = ConvergenceER\ClassMarkers.
/// None = our icons are not available this session (use a stock icon).
pub fn prepare(dll_dir: &Path) -> Option<IconIds> {
    let ours = dll_dir.join("mod").join("menu").join(MOVIE);
    let theirs = dll_dir.join("..").join("mod").join("menu").join(MOVIE);
    if !ours.exists() {
        log!("Icons: {} is missing (run Install.bat); using a stock icon", ours.display());
        return None;
    }
    let src = match fs::read(&theirs) {
        Ok(s) => s,
        Err(e) => {
            log!("Icons: can't read {} ({}); using a stock icon", theirs.display(), e);
            return None;
        }
    };
    let (out, first) = match build(&src) {
        Ok(r) => r,
        Err(e) => {
            log!("Icons: can't add them to the world map ({}); using a stock icon", e);
            let _ = fs::write(&ours, &src); // never leave a stale or broken copy in the way
            return None;
        }
    };
    if fs::read(&ours).ok().as_deref() != Some(&out[..]) {
        if let Err(e) = fs::write(&ours, &out) {
            log!("Icons: can't write {} ({}); using a stock icon", ours.display(), e);
            return None;
        }
    }
    log!("Icons: added as icon ids {}-{}", first, first + ICONS.len() as u16 - 1);
    Some(IconIds { weapon: first, armor: first + 1, rune: first + 2 })
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
        let h = u16::from_le_bytes([b[pos], b[pos + 1]]);
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

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// Tags whose body starts with the character id they define.
fn defines_character(code: u16) -> bool {
    matches!(
        code,
        2 | 6 | 10 | 11 | 20 | 21 | 22 | 32 | 33 | 34 | 35 | 36 | 37 | 39 | 46 | 48 | 60 | 75 | 83 | 84 | 87
            | 90 | 91 | 1009
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

/// Convergence's movie with our icons added -> (new file, icon id of our first icon).
fn build(src: &[u8]) -> Result<(Vec<u8>, u16), String> {
    if src.len() < 12 || &src[..3] != b"GFX" {
        return Err("not an uncompressed GFX movie".into());
    }
    let rect_bits = (src[8] >> 3) as usize;
    let first_tag = 8 + (5 + 4 * rect_bits + 7) / 8 + 4;
    let tags = read_tags(src, first_tag, src.len())?;

    // New character ids above every id the movie defines.
    let max_id = tags
        .iter()
        .filter(|t| defines_character(t.code) && t.len >= 2)
        .map(|t| u16_at(src, t.body))
        .max()
        .unwrap_or(0);
    let first_char = max_id as u32 + 1;
    if first_char + ICONS.len() as u32 > u16::MAX as u32 {
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
    for (i, (_, body)) in ICONS.iter().enumerate() {
        let mut b = ((first_char + i as u32) as u16).to_le_bytes().to_vec();
        b.extend_from_slice(body);
        bitmaps.extend(tag(36, &b, true));
    }

    // The sprite: its own tags (without End), one frame per icon, End.
    let mut body = Vec::new();
    body.extend_from_slice(&ICON_SPRITE.to_le_bytes());
    body.extend_from_slice(&(frames + ICONS.len() as u16).to_le_bytes());
    let end_tag = inner.last().filter(|t| t.code == 0).ok_or("icon sprite has no End tag")?;
    body.extend_from_slice(&src[sprite.body + 4..end_tag.start]);
    for (i, (_, bmp)) in ICONS.iter().enumerate() {
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
    fn build_from_real_movie() {
        let (Ok(src), Ok(out)) = (std::env::var("CM_GFX_SRC"), std::env::var("CM_GFX_OUT")) else {
            return;
        };
        let (bytes, first) = super::build(&std::fs::read(src).unwrap()).unwrap();
        std::fs::write(out, bytes).unwrap();
        println!("first icon id {}", first);
    }
}
