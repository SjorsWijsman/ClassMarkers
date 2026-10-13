//! A text colour per class for the class line in item descriptions, taken from the class's note
//! icon (ClassMarkers/icons/notes/<Class> (path).png, else its weapons icon), so a swapped icon
//! brings its colour along. Used for the class names in item descriptions; the map icons' glow is
//! baked in the same colour by tools/note_icons.py. Lines in ClassMarkers/icons/colors.txt override it:
//!
//! ```text
//! Servant of Rot = #9BC25A
//! ```
//!
//! A class with neither keeps the default text colour.

use crate::generated::NOTES;
use crate::log;
use crate::pixels::decode_png;
use std::collections::HashMap;
use std::path::Path;

const OVERRIDES: &str = "colors.txt";
/// The colour is the average of this share of the icon's pixels, its most vivid ones.
const VIVID_SHARE: usize = 5; // 1/5
/// Limits that keep the colour readable on the dark description box.
const MAX_SATURATION: f32 = 0.65;
const MIN_VALUE: f32 = 0.88;
const MIN_LUMINANCE: f32 = 0.5;

/// Class name -> "#RRGGBB".
pub fn class_colors(dll_dir: &Path) -> HashMap<&'static str, String> {
    let mut out = HashMap::new();
    // Path icons first; a weapons icon only for a class whose path icon gave nothing.
    for which in ["path", "weapons"] {
        for note in NOTES.iter().filter(|n| n.which == which) {
            if out.contains_key(note.class) {
                continue;
            }
            let path = crate::iconcache::icon_path(dll_dir, "notes", &format!("{} ({}).png", note.class, note.which));
            let Ok(bytes) = std::fs::read(path) else { continue };
            if let Some(c) = decode_png(&bytes).ok().and_then(|img| icon_colour(&img.data)) {
                out.insert(note.class, c);
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(dll_dir.join("icons").join(OVERRIDES)) {
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let Some((class, colour)) = line.split_once('=') else { continue };
            let (class, colour) = (class.trim(), colour.trim().to_uppercase());
            let valid = colour.len() == 7 && colour.starts_with('#') && colour[1..].chars().all(|c| c.is_ascii_hexdigit());
            match NOTES.iter().find(|n| n.class.eq_ignore_ascii_case(class)) {
                Some(n) if valid => {
                    out.insert(n.class, colour);
                }
                _ => log!("Class colours: ignored line in {}: {}", OVERRIDES, line),
            }
        }
    }
    out
}

/// The icon's most vivid colour, made readable: RGBA pixels -> "#RRGGBB".
fn icon_colour(rgba: &[u8]) -> Option<String> {
    let mut scored: Vec<(f32, [f32; 3])> = rgba
        .chunks(4)
        .filter(|p| p[3] > 128)
        .map(|p| {
            let c = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
            let (_, s, v) = hsv(c);
            (s * v, c)
        })
        .collect();
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let top = &scored[..(scored.len() / VIVID_SHARE).max(1)];
    let mut avg = [0.0; 3];
    for (_, c) in top {
        for i in 0..3 {
            avg[i] += c[i] / top.len() as f32;
        }
    }
    let (h, s, v) = hsv(avg);
    let mut c = rgb(h, s.min(MAX_SATURATION), v.max(MIN_VALUE));
    // Dark hues (blue, purple, red) still read dim: lighten towards white until bright enough.
    let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let l = lum(c);
    if l < MIN_LUMINANCE {
        let t = (MIN_LUMINANCE - l) / (1.0 - l);
        c = c.map(|x| x + (1.0 - x) * t);
    }
    let b = c.map(|x| (x * 255.0).round().clamp(0.0, 255.0) as u8);
    Some(format!("#{:02X}{:02X}{:02X}", b[0], b[1], b[2]))
}

fn hsv([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

#[cfg(test)]
mod tests {
    /// Prints every class's colour: CM_ICON_DIR=<ClassMarkers folder>.
    #[test]
    #[ignore = "needs real game files, see the CM_* variables above"]
    fn print_colours() {
        let Ok(dir) = std::env::var("CM_ICON_DIR") else { return };
        let mut c: Vec<_> = super::class_colors(std::path::Path::new(&dir)).into_iter().collect();
        c.sort();
        for (class, colour) in c {
            println!("{} {}", class, colour);
        }
    }
}
