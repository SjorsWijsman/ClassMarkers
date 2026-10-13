//! The game's file formats we read and write: DCX (a compressed file), BND4 and BHF4 (binders of
//! files), TPF (textures) and BC7 DDS headers, plus little-endian reads and writes on byte slices.

use std::ops::Range;

pub fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
pub fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
pub fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
pub fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
pub fn put_u64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

// ---- DCX ------------------------------------------------------------------------------------

/// The game's Oodle decompressor, OodleLZ_Decompress (see iconsource.rs oodle).
pub type OodleDecompress = unsafe extern "C" fn(
    *const u8,
    i64,
    *mut u8,
    i64,
    i32,
    i32,
    i32,
    *mut u8,
    i64,
    usize,
    usize,
    usize,
    i64,
    i32,
) -> i64;

/// A DCX file's compression ("DFLT", "KRAK", ...), uncompressed size and compressed data.
fn dcx_parts(file: &[u8]) -> Result<(&[u8], usize, &[u8]), String> {
    if file.len() < 0x4C || &file[..4] != b"DCX\0" {
        return Err("not a DCX file".into());
    }
    let size = u32::from_be_bytes(file[0x1C..0x20].try_into().unwrap()) as usize;
    let csize = u32::from_be_bytes(file[0x20..0x24].try_into().unwrap()) as usize;
    let body = file.get(0x4C..0x4C + csize).ok_or("DCX file is truncated")?;
    Ok((&file[0x28..0x2C], size, body))
}

/// The contents of a zlib (DFLT) DCX file.
pub fn undcx(file: &[u8]) -> Result<Vec<u8>, String> {
    match dcx_parts(file)? {
        (b"DFLT", _, body) => {
            miniz_oxide::inflate::decompress_to_vec_zlib(body).map_err(|e| format!("can't unpack ({:?})", e))
        }
        _ => Err("not a zlib DCX file".into()),
    }
}

/// The contents of a DCX file compressed with zlib (DFLT) or Oodle (KRAK).
pub fn undcx_any(file: &[u8], kraken: OodleDecompress) -> Result<Vec<u8>, String> {
    let (kind, size, body) = dcx_parts(file)?;
    if kind != b"KRAK" {
        return undcx(file);
    }
    let mut out = vec![0u8; size];
    let n = unsafe {
        kraken(
            body.as_ptr(),
            body.len() as i64,
            out.as_mut_ptr(),
            size as i64,
            0,
            0,
            0,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            0,
            3,
        )
    };
    if n as usize != size {
        return Err("Oodle could not unpack it".into());
    }
    Ok(out)
}

/// `data` packed like `template` (same DCX header, new sizes), with zlib at `level`.
pub fn redcx(template: &[u8], data: &[u8], level: u8) -> Vec<u8> {
    let comp = miniz_oxide::deflate::compress_to_vec_zlib(data, level);
    let mut out = template[..0x4C].to_vec();
    out[0x1C..0x20].copy_from_slice(&(data.len() as u32).to_be_bytes());
    out[0x20..0x24].copy_from_slice(&(comp.len() as u32).to_be_bytes());
    out.extend(comp);
    out
}

// ---- BND4 / BHF4 ----------------------------------------------------------------------------

/// One file in a binder: where its header entry is, where its data is, and its name's offset
/// (if the binder has names).
pub struct BndFile {
    pub entry: usize,
    pub data: Range<usize>,
    pub name_at: Option<usize>,
}

/// Layout of a binder header entry for its format flags.
struct EntryLayout {
    /// Offset of the uncompressed size, if there is one.
    usize_at: Option<usize>,
    data_at: usize,
    /// Whether the data offset is 64-bit.
    long: bool,
    name_at: Option<usize>,
}

fn entry_layout(b: &[u8]) -> EntryLayout {
    let fmt = b[0x31].reverse_bits();
    let has_usize = fmt & 0x20 != 0;
    let long = fmt & 0x10 != 0;
    let data_at = if has_usize { 24 } else { 16 };
    let id_size = if fmt & 0x02 != 0 { 4 } else { 0 };
    let name_at = (fmt & 0x0C != 0).then_some(data_at + if long { 8 } else { 4 } + id_size);
    EntryLayout { usize_at: has_usize.then_some(16), data_at, long, name_at }
}

/// The files of a binder with header `magic`: "BND4" (data in the same file) or "BHF4" (a header
/// whose data offsets point into a separate .bdt file, so they aren't checked against `b`).
fn binder(b: &[u8], magic: &[u8; 4]) -> Result<Vec<BndFile>, String> {
    if b.len() < 0x40 || &b[..4] != magic {
        return Err(format!("not a {} binder", String::from_utf8_lossy(magic)));
    }
    let count = u32_at(b, 0x0C) as usize;
    let hsize = u32_at(b, 0x20) as usize;
    let layout = entry_layout(b);
    let mut out = Vec::new();
    for i in 0..count {
        let e = 0x40 + i * hsize;
        let size = u64_at(b, e + 8) as usize;
        let off =
            if layout.long { u64_at(b, e + layout.data_at) as usize } else { u32_at(b, e + layout.data_at) as usize };
        if magic == b"BND4" && off + size > b.len() {
            return Err("binder entry runs past the end".into());
        }
        let name_at = layout.name_at.map(|n| u32_at(b, e + n) as usize);
        out.push(BndFile { entry: e, data: off..off + size, name_at });
    }
    Ok(out)
}

pub fn bnd4(b: &[u8]) -> Result<Vec<BndFile>, String> {
    binder(b, b"BND4")
}

/// A BHF4 header's files as (file name without folders, lowercase) -> data range in the .bdt.
pub fn bhf4(b: &[u8]) -> Result<Vec<(String, Range<usize>)>, String> {
    Ok(binder(b, b"BHF4")?
        .into_iter()
        .filter_map(|f| {
            let name: String = b.get(f.name_at?..)?.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
            Some((name.rsplit('\\').next().unwrap_or("").to_lowercase(), f.data))
        })
        .collect())
}

/// Point binder file `f`'s entry at `len` bytes of data at `at`.
pub fn set_bnd4_data(b: &mut [u8], f: &BndFile, at: u64, len: u64) {
    let layout = entry_layout(b);
    put_u64(b, f.entry + 8, len);
    if let Some(u) = layout.usize_at {
        put_u64(b, f.entry + u, len);
    }
    if layout.long {
        put_u64(b, f.entry + layout.data_at, at);
    } else {
        put_u32(b, f.entry + layout.data_at, at as u32);
    }
}

// ---- TPF and DDS ----------------------------------------------------------------------------

/// A texture in a TPF: its header entry and data.
pub struct Texture {
    pub entry: usize,
    pub name: String,
    pub data: Range<usize>,
}

pub fn tpf(b: &[u8]) -> Result<Vec<Texture>, String> {
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
            let units: Vec<u16> =
                b[no..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
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

/// Size of a DDS header with the DX10 extension; the pixel data follows it.
pub const DDS_HEADER: usize = 148;

/// (width, height) of a BC7 DDS (DX10 header, format BC7 or BC7 sRGB).
pub fn bc7_dds(dds: &[u8]) -> Result<(u32, u32), String> {
    if dds.len() < DDS_HEADER || &dds[0x54..0x58] != b"DX10" || !matches!(u32_at(dds, 0x80), 98 | 99) {
        return Err("texture is not BC7".into());
    }
    Ok((u32_at(dds, 0x10), u32_at(dds, 0x0C)))
}
