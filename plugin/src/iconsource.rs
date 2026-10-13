//! Convergence's own art, read from its menu files (never copied into our files or the repo):
//! - item icons: MENU_ItemIcon_<id> rectangles of the menu atlas (menu/hi/01_common.sblytbnd +
//!   01_common.tpf, BC7), e.g. the class notes';
//! - items' detail images: menu/hi/00_solo.tpfbhd/bdt, menu_knowledge_<id>.tpf.dcx (1024 px BC7,
//!   Oodle-compressed), e.g. every spell rune tier's.

use crate::noteicons::{bnd4, tpf, u32_at, undcx, LAYOUTS, TEXTURES};
use crate::pixels::Rgba;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The value of attribute `name` in an XML tag's text.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.find(&format!(" {}=\"", name))? + name.len() + 3;
    Some(&tag[at..at + tag[at..].find('"')?])
}

/// A rectangle of a BC7 texture (`blocks` = its data after the 148-byte header, `width` pixels
/// wide), decoded to RGBA.
fn bc7_region(blocks: &[u8], width: usize, x: usize, y: usize, w: usize, h: usize) -> Result<Rgba, String> {
    let bw = width / 4;
    let (bx0, by0, bx1, by1) = (x / 4, y / 4, (x + w).div_ceil(4), (y + h).div_ceil(4));
    let pw = (bx1 - bx0) * 4;
    let mut px = vec![0u8; pw * (by1 - by0) * 4 * 4];
    for by in by0..by1 {
        for bx in bx0..bx1 {
            let at = (by * bw + bx) * 16;
            let block = blocks.get(at..at + 16).ok_or("texture data ends early")?;
            let out = ((by - by0) * 4 * pw + (bx - bx0) * 4) * 4;
            bcdec_rs::bc7(block, &mut px[out..], pw * 4);
        }
    }
    let full = Rgba { width: pw as u32, height: ((by1 - by0) * 4) as u32, data: px };
    let (ox, oy) = (x - bx0 * 4, y - by0 * 4);
    Ok(crate::iconart::crop(&full, (ox, oy, ox + w, oy + h)))
}

/// A whole one-level BC7 DDS (with DX10 header) decoded.
fn bc7_dds(dds: &[u8]) -> Result<Rgba, String> {
    let dxgi = if dds.len() >= 148 && &dds[0x54..0x58] == b"DX10" { u32_at(dds, 0x80) } else { 0 };
    if dxgi != 98 && dxgi != 99 {
        return Err("texture is not BC7".into());
    }
    let (h, w) = (u32_at(dds, 0x0C) as usize, u32_at(dds, 0x10) as usize);
    bc7_region(&dds[148..], w, 0, 0, w, h)
}

/// The item icons `wanted` (icon ids) from the menu atlas in `hi` (Convergence's menu/hi).
pub fn atlas_icons(hi: &Path, wanted: &[u16]) -> Result<HashMap<u16, Rgba>, String> {
    let layouts = undcx(&fs::read(hi.join(LAYOUTS)).map_err(|e| e.to_string())?)?;
    let mut rects: HashMap<u16, (String, usize, usize, usize, usize)> = HashMap::new();
    for f in bnd4(&layouts)? {
        let Ok(text) = std::str::from_utf8(&layouts[f.data.clone()]) else { continue };
        let Some(atlas) = text.find("<TextureAtlas").and_then(|i| attr(&text[i..], "imagePath")) else { continue };
        let atlas = atlas.trim_end_matches(".png").to_string();
        for tag in text.split("<SubTexture").skip(1) {
            let tag = &tag[..tag.find("/>").unwrap_or(tag.len())];
            let Some(id) = attr(tag, "name")
                .and_then(|n| n.strip_prefix("MENU_ItemIcon_"))
                .and_then(|n| n.trim_end_matches(".png").parse::<u16>().ok())
            else {
                continue;
            };
            if !wanted.contains(&id) {
                continue;
            }
            let n = |k| attr(tag, k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            rects.insert(id, (atlas.clone(), n("x"), n("y"), n("width"), n("height")));
        }
    }
    let tex = undcx(&fs::read(hi.join(TEXTURES)).map_err(|e| e.to_string())?)?;
    let textures = tpf(&tex)?;
    let mut out = HashMap::new();
    for (id, (atlas, x, y, w, h)) in rects {
        let Some(t) = textures.iter().find(|t| t.name == atlas) else { continue };
        let dds = &tex[t.data.clone()];
        if dds.len() < 148 || &dds[0x54..0x58] != b"DX10" || !matches!(u32_at(dds, 0x80), 98 | 99) {
            continue;
        }
        out.insert(id, bc7_region(&dds[148..], u32_at(dds, 0x10) as usize, x, y, w, h)?);
    }
    Ok(out)
}

// ---- Oodle (KRAK) ---------------------------------------------------------------------------

pub type OodleDecompress = unsafe extern "C" fn(
    *const u8, i64, *mut u8, i64, i32, i32, i32, *mut u8, i64, usize, usize, usize, i64, i32,
) -> i64;

/// The game's Oodle decompressor (oo2core_6_win64.dll, next to eldenring.exe), loaded if needed
/// when `load` (never from DllMain: loading a DLL under the loader lock can deadlock).
pub fn oodle(load: bool) -> Option<OodleDecompress> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress, LoadLibraryA};
    unsafe {
        let dll = c"oo2core_6_win64.dll".as_ptr() as *const u8;
        let mut module = GetModuleHandleA(dll);
        if module.is_null() && load {
            module = LoadLibraryA(dll);
        }
        if module.is_null() {
            return None;
        }
        let f = GetProcAddress(module, c"OodleLZ_Decompress".as_ptr() as *const u8)?;
        Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, OodleDecompress>(f))
    }
}

/// The contents of a DCX file compressed with zlib (DFLT) or Oodle (KRAK).
fn undcx_any(file: &[u8], kraken: OodleDecompress) -> Result<Vec<u8>, String> {
    if file.len() < 0x4C || &file[..4] != b"DCX\0" {
        return Err("not a DCX file".into());
    }
    if &file[0x28..0x2C] != b"KRAK" {
        return undcx(file);
    }
    let usize_ = u32::from_be_bytes(file[0x1C..0x20].try_into().unwrap()) as usize;
    let csize = u32::from_be_bytes(file[0x20..0x24].try_into().unwrap()) as usize;
    let comp = file.get(0x4C..0x4C + csize).ok_or("DCX file is truncated")?;
    let mut out = vec![0u8; usize_];
    let n = unsafe {
        kraken(comp.as_ptr(), csize as i64, out.as_mut_ptr(), usize_ as i64, 0, 0, 0, std::ptr::null_mut(), 0, 0, 0, 0, 0, 3)
    };
    if n as usize != usize_ {
        return Err("Oodle could not unpack it".into());
    }
    Ok(out)
}

/// The header of the binder holding the items' detail images (its data is the .tpfbdt next to it).
pub const DETAIL_IMAGES: &str = "00_solo.tpfbhd";

/// The detail images of the items whose icon ids are `wanted`, from menu/hi/00_solo.
pub fn detail_images(hi: &Path, wanted: &[u16], kraken: OodleDecompress) -> Result<HashMap<u16, Rgba>, String> {
    let bhd = fs::read(hi.join(DETAIL_IMAGES)).map_err(|e| e.to_string())?;
    if bhd.len() < 0x40 || &bhd[..4] != b"BHF4" {
        return Err("00_solo.tpfbhd is not a BHF4 header".into());
    }
    let count = u32_at(&bhd, 0x0C) as usize;
    let hsize = u32_at(&bhd, 0x20) as usize;
    let fmt = bhd[0x31].reverse_bits();
    let off_at = if fmt & 0x20 != 0 { 24 } else { 16 };
    let long = fmt & 0x10 != 0;
    let name_at = off_at + if long { 8 } else { 4 } + if fmt & 0x02 != 0 { 4 } else { 0 };
    let mut index: HashMap<String, (u64, usize)> = HashMap::new();
    for i in 0..count {
        let e = 0x40 + i * hsize;
        let size = u64::from_le_bytes(bhd[e + 8..e + 16].try_into().unwrap()) as usize;
        let off = if long { u64::from_le_bytes(bhd[e + off_at..e + off_at + 8].try_into().unwrap()) } else { u32_at(&bhd, e + off_at) as u64 };
        let no = u32_at(&bhd, e + name_at) as usize;
        let name: String = bhd[no..].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
        let name = name.rsplit('\\').next().unwrap_or("").to_lowercase();
        index.insert(name, (off, size));
    }
    let mut bdt = fs::File::open(hi.join("00_solo.tpfbdt")).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for &id in wanted {
        let Some(&(off, size)) = index.get(&format!("menu_knowledge_{:05}.tpf.dcx", id)) else { continue };
        let mut raw = vec![0u8; size];
        bdt.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
        bdt.read_exact(&mut raw).map_err(|e| e.to_string())?;
        let tex = if &raw[..4] == b"DCX\0" { undcx_any(&raw, kraken)? } else { raw };
        let t = tpf(&tex)?;
        let Some(first) = t.first() else { continue };
        out.insert(id, bc7_dds(&tex[first.data.clone()])?);
    }
    Ok(out)
}
