//! Byte-pattern search over the game executable's code section.

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

/// The executable's `.text` section as a byte slice.
pub fn text_section() -> Option<&'static [u8]> {
    unsafe {
        let base = GetModuleHandleW(std::ptr::null()) as *const u8;
        if base.is_null() {
            return None;
        }
        let nt = base.add(*(base.add(0x3C) as *const u32) as usize);
        let num_sections = *(nt.add(4 + 2) as *const u16) as usize;
        let opt_size = *(nt.add(4 + 16) as *const u16) as usize;
        let sections = nt.add(4 + 20 + opt_size);
        for i in 0..num_sections {
            let s = sections.add(i * 40);
            if std::slice::from_raw_parts(s, 5) == b".text" {
                let size = *(s.add(8) as *const u32) as usize;
                let rva = *(s.add(12) as *const u32) as usize;
                return Some(std::slice::from_raw_parts(base.add(rva), size));
            }
        }
        None
    }
}

fn parse(pattern: &str) -> Vec<Option<u8>> {
    pattern
        .split_whitespace()
        .map(|b| if b == "??" { None } else { Some(u8::from_str_radix(b, 16).unwrap()) })
        .collect()
}

/// Every address where `pattern` matches.
pub fn find_all(pattern: &str) -> Vec<*const u8> {
    let pat = parse(pattern);
    let Some(text) = text_section() else { return vec![] };
    let first = pat[0].expect("pattern must start with a fixed byte");
    let mut out = vec![];
    let last = text.len().saturating_sub(pat.len());
    let mut i = 0;
    while i <= last {
        if text[i] == first && pat.iter().enumerate().all(|(j, p)| p.map_or(true, |b| text[i + j] == b)) {
            out.push(unsafe { text.as_ptr().add(i) });
        }
        i += 1;
    }
    out
}

/// The one address where `pattern` (named `what` in errors) matches. Several matches mean the
/// pattern no longer pins down one place (a game update?), so that is an error too.
pub fn find_unique(what: &str, pattern: &str) -> Result<*const u8, String> {
    match find_all(pattern).as_slice() {
        [hit] => Ok(*hit),
        [] => Err(format!("the {} pattern was not found (unsupported game version?)", what)),
        hits => Err(format!("the {} pattern matched {} places, expected 1 (unsupported game version?)", what, hits.len())),
    }
}

/// The global that a RIP-relative load matching `pattern` reads (see rip_relative). A global is
/// often loaded in several places, so every match must agree on it.
pub fn find_global(what: &str, pattern: &str, disp_offset: usize, next_offset: usize) -> Result<*const u8, String> {
    global_of(what, &find_all(pattern), disp_offset, next_offset)
}

/// find_global for matches already found.
pub fn global_of(what: &str, hits: &[*const u8], disp_offset: usize, next_offset: usize) -> Result<*const u8, String> {
    let targets: std::collections::BTreeSet<usize> =
        hits.iter().map(|&h| unsafe { rip_relative(h, disp_offset, next_offset) } as usize).collect();
    match targets.len() {
        1 => Ok(*targets.first().unwrap() as *const u8),
        0 => Err(format!("the {} pattern was not found (unsupported game version?)", what)),
        n => Err(format!("the {} pattern points at {} different places, expected 1 (unsupported game version?)", what, n)),
    }
}

/// Resolve a RIP-relative operand: the i32 at `at + disp_offset`, relative to `at + next_offset`.
pub unsafe fn rip_relative(at: *const u8, disp_offset: usize, next_offset: usize) -> *const u8 {
    let disp = std::ptr::read_unaligned(at.add(disp_offset) as *const i32);
    at.add(next_offset).offset(disp as isize)
}
