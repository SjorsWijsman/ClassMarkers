//! Our own icons for the class line in item descriptions.
//!
//! The description box draws <img src="img://NAME">, where NAME is an entry of the menu icon atlas:
//! menu/hi/01_common.sblytbnd holds layouts ("NAME is this rectangle of texture T") and
//! menu/hi/01_common.tpf the textures. We write copies of both into our me3 package
//! (ClassMarkers/mod/menu/hi/), which loads after Convergence's: the texture that holds the note
//! icons gets extra rows at the bottom with our icons in them (BC7, like the rest of it), and its
//! layout gets an entry MENU_ItemIcon_CM_<note goods id> for each. Existing entries keep their
//! coordinates, so everything else looks the same. Convergence's files are never touched.
//!
//! The icons are read from ClassMarkers/icons/notes/<Class> (path|weapons).png, so anyone can edit
//! or swap them (tools/note_icons.py exports Convergence's as a start). A note without a file keeps
//! its own icon. Rebuilding takes a few seconds, so it only happens when an icon or Convergence's
//! files changed (a stamp file remembers what the copies were built from). When anything goes
//! wrong the copies are made identical to Convergence's files and every note keeps its own icon.

use crate::generated::NOTES;
use crate::log;
use crate::pixels::{decode_png, Rgba};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

pub(crate) const TEXTURES: &str = "01_common.tpf.dcx";
pub(crate) const LAYOUTS: &str = "01_common.sblytbnd.dcx";
const STAMP: &str = "class_markers_icons.stamp";
/// Prefix of our atlas entries; the note's goods id follows.
pub const NAME_PREFIX: &str = "MENU_ItemIcon_CM_";
/// Empty pixels around each of our icons in the texture (a multiple of 4: one BC7 block).
const GAP: u32 = 4;

/// Must match file_name in tools/note_icons.py.
fn file_name(class: &str, which: &str) -> String {
    format!("{} ({}).png", class, which)
}

/// Note goods id -> (width, height) of our icon, for the notes that have one this session.
pub type Available = HashMap<u32, (u32, u32)>;

/// Bring our copies of the atlas files up to date. `dll_dir` = ConvergenceER\ClassMarkers.
pub fn prepare(dll_dir: &Path) -> Available {
    let ours = dll_dir.join("mod").join("menu").join("hi");
    let theirs = dll_dir.join("..").join("mod").join("menu").join("hi");
    if !ours.join(TEXTURES).exists() || !ours.join(LAYOUTS).exists() {
        log!("Note icons: our copies in {} are missing (run Install.bat); notes keep their own icons", ours.display());
        return Available::new();
    }
    match update(dll_dir, &ours, &theirs) {
        Ok(found) => found,
        Err(e) => {
            log!("Note icons: {}; notes keep their own icons", e);
            let _ = fs::remove_file(ours.join(STAMP));
            for f in [TEXTURES, LAYOUTS] {
                // never leave a stale or broken copy in the way
                if let Err(e) = fs::copy(theirs.join(f), ours.join(f)) {
                    log!("Note icons: can't restore {} ({})", f, e);
                }
            }
            Available::new()
        }
    }
}

fn update(dll_dir: &Path, ours: &Path, theirs: &Path) -> Result<Available, String> {
    // Our icons, and a stamp of everything the copies are built from.
    let mut icons: Vec<(u32, Rgba)> = Vec::new();
    let mut stamp = format!("v{}", env!("CARGO_PKG_VERSION"));
    for f in [TEXTURES, LAYOUTS] {
        let m = fs::metadata(theirs.join(f)).map_err(|e| format!("can't read Convergence's {} ({})", f, e))?;
        stamp += &format!("|{}:{}:{:?}", f, m.len(), m.modified().ok());
    }
    for note in NOTES {
        let path = crate::iconcache::icon_path(dll_dir, "notes", &file_name(note.class, note.which));
        let Ok(bytes) = fs::read(&path) else { continue };
        match decode_png(&bytes) {
            Ok(img) => {
                stamp += &format!("|{}:{:016x}", note.goods_id, fnv(&bytes));
                icons.push((note.goods_id, img));
            }
            Err(e) => log!("Note icons: can't read {} ({}); that note keeps its own icon", path.display(), e),
        }
    }
    let found: Available = icons.iter().map(|(id, img)| (*id, (img.width, img.height))).collect();
    if fs::read_to_string(ours.join(STAMP)).ok().as_deref() == Some(stamp.as_str()) {
        log!("Note icons: {} in use, unchanged", found.len());
        return Ok(found);
    }

    let started = Instant::now();
    let (textures, layouts) = if icons.is_empty() {
        (fs::read(theirs.join(TEXTURES)).map_err(|e| e.to_string())?, fs::read(theirs.join(LAYOUTS)).map_err(|e| e.to_string())?)
    } else {
        build(&fs::read(theirs.join(TEXTURES)).map_err(|e| e.to_string())?,
              &fs::read(theirs.join(LAYOUTS)).map_err(|e| e.to_string())?,
              &icons)?
    };
    for (f, data) in [(TEXTURES, &textures), (LAYOUTS, &layouts)] {
        fs::write(ours.join(f), data).map_err(|e| format!("can't write our {} ({})", f, e))?;
    }
    fs::write(ours.join(STAMP), &stamp).map_err(|e| format!("can't write {} ({})", STAMP, e))?;
    log!("Note icons: {} added to the menu icons in {:.1} s", found.len(), started.elapsed().as_secs_f32());
    Ok(found)
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325, |h, &x| (h ^ x as u64).wrapping_mul(0x100_0000_01b3))
}

pub(crate) fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_u64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

/// The contents of a zlib (DFLT) DCX file.
pub(crate) fn undcx(file: &[u8]) -> Result<Vec<u8>, String> {
    if file.len() < 0x4C || &file[..4] != b"DCX\0" || &file[0x28..0x2C] != b"DFLT" {
        return Err("not a zlib DCX file".into());
    }
    let csize = u32::from_be_bytes(file[0x20..0x24].try_into().unwrap()) as usize;
    let body = file.get(0x4C..0x4C + csize).ok_or("DCX file is truncated")?;
    miniz_oxide::inflate::decompress_to_vec_zlib(body).map_err(|e| format!("can't unpack ({:?})", e))
}

/// `data` packed like `template` (same DCX header, new sizes).
fn redcx(template: &[u8], data: &[u8], level: u8) -> Vec<u8> {
    let comp = miniz_oxide::deflate::compress_to_vec_zlib(data, level);
    let mut out = template[..0x4C].to_vec();
    out[0x1C..0x20].copy_from_slice(&(data.len() as u32).to_be_bytes());
    out[0x20..0x24].copy_from_slice(&(comp.len() as u32).to_be_bytes());
    out.extend(comp);
    out
}

/// One file in a BND4 binder: where its header entry is and where its data is.
pub(crate) struct BndFile {
    entry: usize,
    pub(crate) data: std::ops::Range<usize>,
}

/// Layout of a BND4 header entry for its format flags: (offset of the uncompressed size if any,
/// offset of the data offset, whether that is 64-bit).
fn bnd4_fields(fmt: u8) -> (Option<usize>, usize, bool) {
    let has_usize = fmt & 0x20 != 0;
    (has_usize.then_some(16), if has_usize { 24 } else { 16 }, fmt & 0x10 != 0)
}

pub(crate) fn bnd4(b: &[u8]) -> Result<Vec<BndFile>, String> {
    if b.len() < 0x40 || &b[..4] != b"BND4" {
        return Err("not a BND4 binder".into());
    }
    let count = u32_at(b, 0x0C) as usize;
    let hsize = u32_at(b, 0x20) as usize;
    let (_, off_at, long) = bnd4_fields(b[0x31].reverse_bits());
    let mut out = Vec::new();
    for i in 0..count {
        let e = 0x40 + i * hsize;
        let size = u64::from_le_bytes(b[e + 8..e + 16].try_into().unwrap()) as usize;
        let off = if long { u64::from_le_bytes(b[e + off_at..e + off_at + 8].try_into().unwrap()) as usize } else { u32_at(b, e + off_at) as usize };
        if off + size > b.len() {
            return Err("binder entry runs past the end".into());
        }
        out.push(BndFile { entry: e, data: off..off + size });
    }
    Ok(out)
}

/// A texture in a TPF: its header entry and data.
pub(crate) struct Texture {
    entry: usize,
    pub(crate) name: String,
    pub(crate) data: std::ops::Range<usize>,
}

pub(crate) fn tpf(b: &[u8]) -> Result<Vec<Texture>, String> {
    if b.len() < 0x10 || &b[..4] != b"TPF\0" {
        return Err("not a TPF texture file".into());
    }
    let count = u32_at(b, 0x08) as usize;
    let utf16 = b[0x0E] == 1;
    let mut out = Vec::new();
    for i in 0..count {
        let e = 0x10 + i * 0x14;
        let (off, size, no) = (u32_at(b, e) as usize, u32_at(b, e + 4) as usize, u32_at(b, e + 0x0C) as usize);
        let name = if utf16 {
            let units: Vec<u16> = b[no..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
            String::from_utf16_lossy(&units)
        } else {
            b[no..].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
        };
        if off + size > b.len() {
            return Err("texture runs past the end".into());
        }
        out.push(Texture { entry: e, name, data: off..off + size });
    }
    Ok(out)
}

/// Convergence's atlas files with our icons added -> (textures file, layouts file).
fn build(textures_dcx: &[u8], layouts_dcx: &[u8], icons: &[(u32, Rgba)]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let bnd = undcx(layouts_dcx)?;
    let files = bnd4(&bnd)?;

    // The layout that holds the notes' own icons; its texture gets ours too.
    let (layout, atlas) = NOTES
        .iter()
        .find_map(|n| {
            let needle = format!("name=\"MENU_ItemIcon_{:05}.png\"", n.icon);
            files.iter().find_map(|f| {
                let text = std::str::from_utf8(&bnd[f.data.clone()]).ok()?;
                if !text.contains(&needle) {
                    return None;
                }
                let at = text.find("imagePath=\"")? + 11;
                let path = &text[at..at + text[at..].find('"')?];
                Some((f, path.trim_end_matches(".png").to_string()))
            })
        })
        .ok_or("the note icons' layout was not found")?;

    let tex = undcx(textures_dcx)?;
    let textures = tpf(&tex)?;
    let target = textures.iter().find(|t| t.name == atlas).ok_or(format!("texture {} not found", atlas))?;
    let dds = &tex[target.data.clone()];
    // One mip level of BC7 with a DX10 header: 148 header bytes, then one byte per pixel.
    let dxgi = if dds.len() >= 148 && &dds[0x54..0x58] == b"DX10" { u32_at(dds, 0x80) } else { 0 };
    if dxgi != 98 && dxgi != 99 {
        return Err(format!("texture {} is not BC7", atlas));
    }
    let (height, width) = (u32_at(dds, 0x0C), u32_at(dds, 0x10));
    if dds.len() != 148 + (width * height) as usize || width % 4 != 0 || height % 4 != 0 {
        return Err(format!("texture {} has an unexpected size or mip levels", atlas));
    }

    // Our icons in rows below the existing image, each on a 4 px grid with a gap around it.
    let round4 = |v: u32| (v + 3) / 4 * 4;
    let mut placed = Vec::new(); // (note, x, y in the new texture, w, h)
    let (mut x, mut y, mut row_h) = (GAP, GAP, 0);
    for (note, img) in icons {
        if img.width + 2 * GAP > width {
            return Err(format!("icon for note {} is wider than the texture", note));
        }
        if x + img.width + GAP > width {
            x = GAP;
            y += row_h + GAP;
            row_h = 0;
        }
        placed.push((*note, x, y, img));
        x += round4(img.width) + GAP;
        row_h = row_h.max(round4(img.height));
    }
    let extra_h = round4(y + row_h + GAP);
    let mut surface = vec![0u8; (width * extra_h * 4) as usize];
    for (_, px, py, img) in &placed {
        for row in 0..img.height {
            let src = (row * img.width * 4) as usize;
            let dst = (((py + row) * width + px) * 4) as usize;
            surface[dst..dst + (img.width * 4) as usize].copy_from_slice(&img.data[src..src + (img.width * 4) as usize]);
        }
    }
    let blocks = intel_tex_2::bc7::compress_blocks(
        &intel_tex_2::bc7::alpha_basic_settings(),
        &intel_tex_2::RgbaSurface { data: &surface, width, height: extra_h, stride: width * 4 },
    );

    let mut new_dds = dds.to_vec();
    new_dds.extend(blocks);
    let new_height = height + extra_h;
    put_u32(&mut new_dds, 0x0C, new_height);
    put_u32(&mut new_dds, 0x14, width * new_height); // linear size of the top level

    // The textures file again, with the bigger texture; the others move along.
    let first_data = textures.iter().map(|t| t.data.start).min().unwrap_or(tex.len());
    let mut out = tex[..first_data].to_vec();
    let mut order: Vec<&Texture> = textures.iter().collect();
    order.sort_by_key(|t| t.data.start);
    let mut total = 0usize;
    let mut body = Vec::with_capacity(tex.len() - first_data + new_dds.len() - dds.len());
    for t in order {
        let data: &[u8] = if t.entry == target.entry { &new_dds } else { &tex[t.data.clone()] };
        put_u32(&mut out, t.entry, (first_data + body.len()) as u32);
        put_u32(&mut out, t.entry + 4, data.len() as u32);
        body.extend_from_slice(data);
        total += data.len();
    }
    put_u32(&mut out, 0x04, total as u32);
    out.extend(body);

    // The layout with our entries; its new text goes at the end of the binder.
    let text = std::str::from_utf8(&bnd[layout.data.clone()]).map_err(|_| "layout is not text")?;
    let close = text.rfind("</TextureAtlas>").ok_or("layout has no </TextureAtlas>")?;
    let mut new_text = text[..close].to_string();
    for (note, px, py, img) in &placed {
        new_text += &format!(
            "  <SubTexture name=\"{}{}.png\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" half=\"0\" />\r\n",
            NAME_PREFIX, note, px, height + py, img.width, img.height
        );
    }
    new_text += &text[close..];
    let mut new_bnd = bnd.clone();
    let fmt = bnd[0x31].reverse_bits();
    let (usize_at, off_at, long) = bnd4_fields(fmt);
    let (e, at, len) = (layout.entry, new_bnd.len() as u64, new_text.len() as u64);
    put_u64(&mut new_bnd, e + 8, len);
    if let Some(u) = usize_at {
        put_u64(&mut new_bnd, e + u, len);
    }
    if long {
        put_u64(&mut new_bnd, e + off_at, at);
    } else {
        put_u32(&mut new_bnd, e + off_at, at as u32);
    }
    new_bnd.extend_from_slice(new_text.as_bytes());

    Ok((redcx(textures_dcx, &out, 1), redcx(layouts_dcx, &new_bnd, 6)))
}

#[cfg(test)]
mod tests {
    /// Builds from real files: CM_ATLAS_SRC=<Convergence's menu\hi> CM_ATLAS_OUT=<folder>
    /// CM_ICONS=<icons\notes>.
    #[test]
    #[ignore = "needs real game files, see the CM_* variables above"]
    fn build_from_real_files() {
        let (Ok(src), Ok(out), Ok(icons)) =
            (std::env::var("CM_ATLAS_SRC"), std::env::var("CM_ATLAS_OUT"), std::env::var("CM_ICONS"))
        else {
            return;
        };
        let src = std::path::Path::new(&src);
        let mut list = Vec::new();
        for n in crate::generated::NOTES {
            if let Ok(b) = std::fs::read(std::path::Path::new(&icons).join(super::file_name(n.class, n.which))) {
                list.push((n.goods_id, crate::pixels::decode_png(&b).unwrap()));
            }
        }
        let t = std::time::Instant::now();
        let (tex, lyt) = super::build(
            &std::fs::read(src.join(super::TEXTURES)).unwrap(),
            &std::fs::read(src.join(super::LAYOUTS)).unwrap(),
            &list,
        )
        .unwrap();
        let out = std::path::Path::new(&out);
        std::fs::write(out.join(super::TEXTURES), tex).unwrap();
        std::fs::write(out.join(super::LAYOUTS), lyt).unwrap();
        println!("{} icons in {:.1} s", list.len(), t.elapsed().as_secs_f32());
    }
}
