//! Making our icons from Convergence's art (see iconcache.rs): cutting a class note to its paper,
//! framing it for text or for the map, and the map icons' outline and glow.

use crate::pixels::Rgba;

/// Rows narrower than this share of a note's widest row are its hanging tail, not its paper.
const TAIL_WIDTH: f32 = 0.15;
/// Text icons: the note's widest side as a share of the square's side, resting on its bottom edge
/// (a text box puts an image's bottom on the baseline), and the square's side.
const ART_SHARE: f32 = 2.0 / 3.0;
pub const TEXT_ICON_PX: u32 = 96;
/// Map icons: room around the art for outline and glow (share of its square, each edge), the
/// outline's colour and softness, and the glow's blur radius (share of the icon) and strength.
const MAP_PAD: f32 = 0.14;
const OUTLINE: [u8; 3] = [12, 10, 8];
const OUTLINE_SOFT: f32 = 0.5;
const GLOW_RADIUS: f32 = 0.035;
const GLOW_GAIN: f32 = 1.6;
/// Rune art is scaled to this before outline and glow (the note map icons' size), and fades out in
/// a dark haze: its outline follows only pixels more opaque than RUNE_SOLID.
pub const RUNE_PX: u32 = 168;
pub const RUNE_SOLID: u8 = 160;
const NOTE_SOLID: u8 = 40;

type Box = (usize, usize, usize, usize); // x0, y0, x1, y1 (exclusive)

fn alpha_box(img: &Rgba, min: u8, rows: usize) -> Option<Box> {
    let w = img.width as usize;
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..rows {
        for x in 0..w {
            if img.data[(y * w + x) * 4 + 3] > min {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    (x0 < x1).then_some((x0, y0, x1, y1))
}

/// The box of a note's paper, without the tail hanging below it: going down from the widest row,
/// the paper ends at the first row narrower than TAIL_WIDTH of it.
fn paper_box(img: &Rgba) -> Option<Box> {
    let (w, h) = (img.width as usize, img.height as usize);
    let widths: Vec<usize> = (0..h).map(|y| (0..w).filter(|&x| img.data[(y * w + x) * 4 + 3] > 16).count()).collect();
    let widest = (0..h).max_by_key(|&y| widths[y])?;
    let limit = widths[widest] as f32 * TAIL_WIDTH;
    let bottom = (widest..h).find(|&y| (widths[y] as f32) < limit).unwrap_or(h);
    alpha_box(img, 16, bottom)
}

pub fn crop(img: &Rgba, (x0, y0, x1, y1): Box) -> Rgba {
    let w = img.width as usize;
    let mut data = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        data.extend_from_slice(&img.data[(y * w + x0) * 4..(y * w + x1) * 4]);
    }
    Rgba { width: (x1 - x0) as u32, height: (y1 - y0) as u32, data }
}

/// The art without transparent margins.
pub fn trim(img: &Rgba) -> Rgba {
    match alpha_box(img, 0, img.height as usize) {
        Some(b) => crop(img, b),
        None => Rgba { width: img.width, height: img.height, data: img.data.clone() },
    }
}

/// Lanczos resize, on premultiplied colour so transparent edges don't darken. (The image crate's
/// float pixels are 0-1: it clamps to that.)
pub fn resize(img: &Rgba, width: u32, height: u32) -> Rgba {
    use image::{imageops, ImageBuffer, Rgba as Px};
    let pre: Vec<f32> = img
        .data
        .chunks(4)
        .flat_map(|p| {
            let a = p[3] as f32 / 255.0;
            [p[0] as f32 / 255.0 * a, p[1] as f32 / 255.0 * a, p[2] as f32 / 255.0 * a, a]
        })
        .collect();
    let buf: ImageBuffer<Px<f32>, Vec<f32>> = ImageBuffer::from_raw(img.width, img.height, pre).unwrap();
    let out = imageops::resize(&buf, width.max(1), height.max(1), imageops::FilterType::Lanczos3);
    let data = out
        .into_raw()
        .chunks(4)
        .flat_map(|p| {
            let a = p[3].clamp(0.0, 1.0);
            let un = |c: f32| if a > 0.0 { (c / a * 255.0).clamp(0.0, 255.0).round() as u8 } else { 0 };
            [un(p[0]), un(p[1]), un(p[2]), (a * 255.0).round() as u8]
        })
        .collect();
    Rgba { width: width.max(1), height: height.max(1), data }
}

fn scale_to(img: &Rgba, side: f32) -> Rgba {
    let s = side / img.width.max(img.height) as f32;
    resize(img, (img.width as f32 * s).round() as u32, (img.height as f32 * s).round() as u32)
}

/// `art` drawn at (x, y) on a transparent `side` square.
fn place(art: &Rgba, side: usize, x: usize, y: usize) -> Rgba {
    let mut data = vec![0u8; side * side * 4];
    let aw = art.width as usize;
    for row in 0..art.height as usize {
        let (sy, dy) = (row, y + row);
        if dy >= side {
            break;
        }
        let n = aw.min(side.saturating_sub(x));
        data[(dy * side + x) * 4..(dy * side + x + n) * 4].copy_from_slice(&art.data[sy * aw * 4..(sy * aw + n) * 4]);
    }
    Rgba { width: side as u32, height: side as u32, data }
}

/// For text: a note (without its tail) at ART_SHARE of a TEXT_ICON_PX square, on its bottom edge,
/// so it sits on the text baseline like a letter.
pub fn text_icon(note: &Rgba) -> Rgba {
    let paper = paper_box(note).map(|b| crop(note, b)).unwrap_or_else(|| trim(note));
    let art = scale_to(&paper, TEXT_ICON_PX as f32 * ART_SHARE);
    let side = TEXT_ICON_PX as usize;
    place(&art, side, (side - art.width as usize) / 2, side - art.height as usize)
}

/// For the map, a class note: the whole note, tail included, framed so the centre of its paper is
/// the image's centre, with outline and glow.
pub fn map_note_icon(note: &Rgba, glow: [u8; 3]) -> Rgba {
    let art = alpha_box(note, 16, note.height as usize).unwrap_or((0, 0, note.width as usize, note.height as usize));
    let paper = paper_box(note).unwrap_or(art);
    let centre = ((paper.0 + paper.2) as f32 / 2.0, (paper.1 + paper.3) as f32 / 2.0);
    map_icon(note, art, centre, glow, NOTE_SOLID)
}

/// For the map, a rune: its art scaled to RUNE_PX, centred, with outline and glow.
pub fn map_rune_icon(rune: &Rgba, glow: [u8; 3]) -> Rgba {
    let art = scale_to(&trim(rune), RUNE_PX as f32);
    let b = (0, 0, art.width as usize, art.height as usize);
    map_icon(&art, b, (art.width as f32 / 2.0, art.height as f32 / 2.0), glow, RUNE_SOLID)
}

/// `img` on the smallest square around `centre` holding its `art` box, plus room; an outline
/// around the pixels more opaque than `solid`, and a glow in `glow` colour outside it.
fn map_icon(img: &Rgba, art: Box, (cx, cy): (f32, f32), glow: [u8; 3], solid: u8) -> Rgba {
    let reach = (cx - art.0 as f32).max(art.2 as f32 - cx).max(cy - art.1 as f32).max(art.3 as f32 - cy);
    let side = 2 * reach.ceil() as usize + 2;
    let pad = (side as f32 * MAP_PAD).round() as usize;
    let full = side + 2 * pad;
    let (ox, oy) = ((full as f32 / 2.0 - cx).round() as isize, (full as f32 / 2.0 - cy).round() as isize);
    let mut note = vec![0u8; full * full * 4];
    let w = img.width as usize;
    for y in 0..img.height as usize {
        for x in 0..w {
            let (dx, dy) = (x as isize + ox, y as isize + oy);
            if dx >= 0 && dy >= 0 && (dx as usize) < full && (dy as usize) < full {
                let d = (dy as usize * full + dx as usize) * 4;
                note[d..d + 4].copy_from_slice(&img.data[(y * w + x) * 4..(y * w + x) * 4 + 4]);
            }
        }
    }
    // Outline: the art's shape grown by a pixel, softened a little.
    let shape: Vec<f32> = note.chunks(4).map(|p| if p[3] > solid { 1.0 } else { 0.0 }).collect();
    let mut rim = vec![0f32; full * full];
    for y in 0..full {
        for x in 0..full {
            let near = (y.saturating_sub(1)..(y + 2).min(full))
                .any(|yy| (x.saturating_sub(1)..(x + 2).min(full)).any(|xx| shape[yy * full + xx] > 0.0));
            rim[y * full + x] = if near { 1.0 } else { 0.0 };
        }
    }
    let rim: Vec<f32> = gaussian(&rim, full, OUTLINE_SOFT).iter().map(|v| (v * 1.3).min(1.0)).collect();
    let halo: Vec<f32> = gaussian(&rim, full, full as f32 * GLOW_RADIUS).iter().map(|v| (v * GLOW_GAIN).min(1.0)).collect();

    // Glow, then outline, then the art.
    let mut out = vec![0u8; full * full * 4];
    for i in 0..full * full {
        let mut px = [glow[0] as f32, glow[1] as f32, glow[2] as f32, halo[i]];
        px = over([OUTLINE[0] as f32, OUTLINE[1] as f32, OUTLINE[2] as f32, rim[i]], px);
        let a = &note[i * 4..i * 4 + 4];
        px = over([a[0] as f32, a[1] as f32, a[2] as f32, a[3] as f32 / 255.0], px);
        out[i * 4..i * 4 + 4].copy_from_slice(&[
            px[0].round() as u8,
            px[1].round() as u8,
            px[2].round() as u8,
            (px[3] * 255.0).round() as u8,
        ]);
    }
    Rgba { width: full as u32, height: full as u32, data: out }
}

/// `top` over `bottom` (straight colour 0-255, alpha 0-1).
fn over(top: [f32; 4], bottom: [f32; 4]) -> [f32; 4] {
    let a = top[3] + bottom[3] * (1.0 - top[3]);
    if a <= 0.0 {
        return [0.0; 4];
    }
    let c = |i: usize| (top[i] * top[3] + bottom[i] * bottom[3] * (1.0 - top[3])) / a;
    [c(0), c(1), c(2), a]
}

/// A Gaussian blur with standard deviation `sigma`, as three box blurs.
fn gaussian(src: &[f32], side: usize, sigma: f32) -> Vec<f32> {
    let r = ((((12.0 * sigma * sigma / 3.0) + 1.0).sqrt() - 1.0) / 2.0).round().max(0.0) as usize;
    if r == 0 {
        return src.to_vec();
    }
    let mut v = src.to_vec();
    for _ in 0..3 {
        v = box_blur(&v, side, r);
    }
    v
}

fn box_blur(src: &[f32], side: usize, r: usize) -> Vec<f32> {
    let pass = |src: &[f32], at: &dyn Fn(usize, usize) -> usize| {
        let mut out = vec![0f32; src.len()];
        let n = (2 * r + 1) as f32;
        for line in 0..side {
            let mut sum: f32 = (0..=r.min(side - 1)).map(|k| src[at(line, k)]).sum();
            for k in 0..side {
                out[at(line, k)] = sum / n;
                if k + r + 1 < side {
                    sum += src[at(line, k + r + 1)];
                }
                if k >= r {
                    sum -= src[at(line, k - r)];
                }
            }
        }
        out
    };
    let rows = pass(src, &|line, k| line * side + k);
    pass(&rows, &|line, k| k * side + line)
}
