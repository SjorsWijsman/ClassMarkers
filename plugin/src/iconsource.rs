//! Convergence's own art, read from its menu files (never copied into our files or the repo):
//! - item icons: MENU_ItemIcon_<id> rectangles of the menu atlas (menu/hi/01_common.sblytbnd +
//!   01_common.tpf, BC7), e.g. the class notes';
//! - items' detail images: menu/hi/00_solo.tpfbhd/bdt, menu_knowledge_<id>.tpf.dcx (1024 px BC7,
//!   Oodle-compressed), e.g. every spell rune tier's.

use crate::formats::{bc7_dds, bhf4, bnd4, tpf, undcx, undcx_any, OodleDecompress, DDS_HEADER};
use crate::noteicons::{LAYOUTS, TEXTURES};
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

/// A rectangle of a BC7 texture (`blocks` = its data after the DDS header, `width` pixels wide),
/// decoded to RGBA.
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

/// A whole one-level BC7 DDS decoded.
fn decode_dds(dds: &[u8]) -> Result<Rgba, String> {
    let (w, h) = bc7_dds(dds)?;
    bc7_region(&dds[DDS_HEADER..], w as usize, 0, 0, w as usize, h as usize)
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
        let Ok((width, _)) = bc7_dds(dds) else { continue };
        out.insert(id, bc7_region(&dds[DDS_HEADER..], width as usize, x, y, w, h)?);
    }
    Ok(out)
}

// ---- Oodle (KRAK) ---------------------------------------------------------------------------

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

/// The header of the binder holding the items' detail images (its data is the .tpfbdt next to it).
pub const DETAIL_IMAGES: &str = "00_solo.tpfbhd";

/// The detail images of the items whose icon ids are `wanted`, from menu/hi/00_solo.
pub fn detail_images(hi: &Path, wanted: &[u16], kraken: OodleDecompress) -> Result<HashMap<u16, Rgba>, String> {
    let bhd = fs::read(hi.join(DETAIL_IMAGES)).map_err(|e| e.to_string())?;
    let index: HashMap<String, std::ops::Range<usize>> = bhf4(&bhd)?.into_iter().collect();
    let mut bdt = fs::File::open(hi.join("00_solo.tpfbdt")).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for &id in wanted {
        let Some(range) = index.get(&format!("menu_knowledge_{:05}.tpf.dcx", id)) else { continue };
        let mut raw = vec![0u8; range.len()];
        bdt.seek(SeekFrom::Start(range.start as u64)).map_err(|e| e.to_string())?;
        bdt.read_exact(&mut raw).map_err(|e| e.to_string())?;
        let tex = if &raw[..4] == b"DCX\0" { undcx_any(&raw, kraken)? } else { raw };
        let t = tpf(&tex)?;
        let Some(first) = t.first() else { continue };
        out.insert(id, decode_dds(&tex[first.data.clone()])?);
    }
    Ok(out)
}
