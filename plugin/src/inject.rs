//! Grow WorldMapPointParam in memory with our marker rows.
//!
//! The param table the game loaded from regulation.bin is copied into a larger buffer with our
//! rows added, and the param's file pointer is switched to the copy. Layout details, the
//! wrapper header and the allocator choice follow Map For Goblins - DLL (goblin_inject.cpp,
//! goblin_gfx_probe.cpp), which ships this technique for Convergence 3.

use crate::log;
use crate::markers::{Row, ROW_SIZE};
use std::collections::HashSet;
use crate::scan;
use std::ffi::c_void;
use std::time::Duration;

/// `mov rcx, [rip+X]` loading the global param list (SoloParamRepository's ParamList*).
const PARAM_LIST_AOB: &str = "48 8B 0D ?? ?? ?? ?? 48 85 C9 0F 84 ?? ?? ?? ?? 45 33 C0 BA 90";
/// The slice of DLNew's operator delete that calls the "which allocator owns this pointer"
/// lookup; the call's rel32 sits at +0x1A.
const DL_FIND_OWNER_AOB: &str = "48 8B 5F 20 48 C7 47 20 00 00 00 00 48 8B 7C 24 38 48 85 DB 75 26 \
    48 8B CE E8 ?? ?? ?? ?? 48 8B D8 48 85 C0 75 16 4C 8D 05 ?? ?? ?? ?? \
    8D 50 64 48 8D 0D ?? ?? ?? ?? E8 ?? ?? ?? ?? 4C 8B 03 48 8B D6 \
    48 8B CB 41 FF 50 68";

const PARAM_LIST_ENTRIES_OFFSET: usize = 136;
const PARAM_LIST_ENTRY_STRIDE: usize = 72;
const PARAM_LIST_ENTRY_COUNT: usize = 186;

const HEADER_SIZE: usize = 0x40;
const ROW_LOCATOR_SIZE: usize = 24; // row_id u64, data offset u64, end offset u64
const WRAPPER_HEADER: usize = 0x10;
const WRAPPER_ROW_LOC_SIZE: usize = 8; // row i32, index i32

unsafe fn rd<T: Copy>(p: *const u8) -> T {
    std::ptr::read_unaligned(p as *const T)
}
unsafe fn wr<T>(p: *mut u8, v: T) {
    std::ptr::write_unaligned(p as *mut T, v)
}

/// DLWString (the engine's std::wstring): inline buffer of 8 wchar_t or a pointer,
/// then length and capacity.
unsafe fn read_dlwstring(p: *const u8) -> String {
    let len: u64 = rd(p.add(16));
    let cap: u64 = rd(p.add(24));
    if len > 256 {
        return String::new();
    }
    let chars = if cap > 7 { rd::<*const u16>(p) } else { p as *const u16 };
    String::from_utf16_lossy(std::slice::from_raw_parts(chars, len as usize))
}

/// Wait for the game to finish creating its params, then return the ParamList.
fn wait_for_param_list() -> Result<*const u8, String> {
    let mut list_slot: Option<*const *const u8> = None;
    for second in 0..300 {
        if list_slot.is_none() {
            if let Some(&hit) = scan::find_all(PARAM_LIST_AOB).first() {
                list_slot = Some(unsafe { scan::rip_relative(hit, 3, 7) } as *const *const u8);
                log!("Found the param list pointer");
            }
        }
        if let Some(slot) = list_slot {
            let list = unsafe { *slot };
            if !list.is_null() {
                let filled = (0..PARAM_LIST_ENTRY_COUNT)
                    .filter(|i| unsafe {
                        !rd::<*const u8>(list.add(PARAM_LIST_ENTRIES_OFFSET + i * PARAM_LIST_ENTRY_STRIDE))
                            .is_null()
                    })
                    .count();
                if filled == PARAM_LIST_ENTRY_COUNT {
                    log!("Params ready after {} s", second);
                    return Ok(list);
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err(match list_slot {
        None => "the param list pattern was not found in the game (unsupported game version?)".into(),
        Some(_) => "the game's params were not ready after 5 minutes".into(),
    })
}

/// The param header (FD4ParamResCap) for a param, by name.
unsafe fn find_param_header(list: *const u8, name: &str) -> Option<*mut u8> {
    for i in 0..PARAM_LIST_ENTRY_COUNT {
        let res_cap: *const u8 = rd(list.add(PARAM_LIST_ENTRIES_OFFSET + i * PARAM_LIST_ENTRY_STRIDE));
        if !res_cap.is_null() && read_dlwstring(res_cap.add(0x18)) == name {
            return Some(rd(res_cap.add(0x80)));
        }
    }
    None
}

/// Allocate `size` bytes (16-aligned, zeroed) from the engine allocator that owns `neighbor`,
/// so the engine can free our buffer exactly like the one it replaces.
pub unsafe fn alloc_like(neighbor: *const u8, size: usize) -> Result<*mut u8, String> {
    let hits = scan::find_all(DL_FIND_OWNER_AOB);
    if hits.len() != 1 {
        return Err(format!("allocator lookup pattern matched {} times, expected 1", hits.len()));
    }
    let find_owner: extern "C" fn(*const c_void) -> *mut c_void =
        std::mem::transmute(scan::rip_relative(hits[0], 0x1A, 0x1E));
    let allocator = find_owner(neighbor as *const c_void);
    if allocator.is_null() {
        return Err("no engine allocator owns the original marker table".into());
    }
    // DLAllocator vtable: +0x50 = allocate_aligned(self, size, alignment)
    let vtable: *const usize = rd(allocator as *const u8);
    let allocate_aligned: extern "C" fn(*mut c_void, usize, usize) -> *mut u8 =
        std::mem::transmute(*vtable.add(0x50 / 8));
    let p = allocate_aligned(allocator, size, 16);
    if p.is_null() || (p as usize) & 15 != 0 {
        return Err(format!("engine allocator refused {} bytes", size));
    }
    std::ptr::write_bytes(p, 0, size);
    Ok(p)
}

/// First two-digit row number we use inside a map block (Convergence's own rows use 00-19).
const FIRST_SLOT: i32 = 20;

/// Add `markers` (map block, row, log label) to the world map. Returns, in `markers` order, the
/// address of each row's data in the live table (None if its map block had no free row id).
pub fn inject(markers: Vec<(i32, Row, String)>) -> Result<Vec<Option<usize>>, String> {
    let list = wait_for_param_list()?;
    unsafe {
        let header = find_param_header(list, "WorldMapPointParam").ok_or("WorldMapPointParam not found")?;
        let file_ptr_ref = header.add(0x80) as *mut *mut u8;
        let file_size_ref = header.add(0x78) as *mut i64;
        let old = *file_ptr_ref;
        let old_rows: u16 = rd(old.add(0x0A));
        let type_off: u64 = rd(old.add(0x10));
        log!("WorldMapPointParam has {} rows", old_rows);

        // Gather (row id, data) for the existing rows plus ours, sorted by id.
        let mut rows: Vec<(i32, *const u8)> = (0..old_rows as usize)
            .map(|i| {
                let loc = old.add(HEADER_SIZE + i * ROW_LOCATOR_SIZE);
                (rd::<u64>(loc) as i32, old.add(rd::<u64>(loc.add(8)) as usize) as *const u8)
            })
            .collect();
        // The world map only draws rows whose id encodes their map block, so each marker gets
        // block * 100 + the first free two-digit number in that block.
        let mut taken: HashSet<i32> = rows.iter().map(|(id, _)| *id).collect();
        let mut ours: Vec<(i32, [u8; ROW_SIZE])> = vec![];
        let mut ids: Vec<Option<i32>> = vec![];
        for (block, row, label) in &markers {
            // Prefer numbers above Convergence's own (00-19); use any free one if the block is busy.
            let free = (FIRST_SLOT..100).chain(0..FIRST_SLOT).map(|n| block * 100 + n).find(|id| !taken.contains(id));
            match free {
                Some(id) => {
                    taken.insert(id);
                    ours.push((id, row.encode()));
                    log!("  row {}: {}", id, label);
                }
                None => log!("  SKIPPED (map block {} is full): {}", block, label),
            }
            ids.push(free);
        }
        for (id, data) in &ours {
            rows.push((*id, data.as_ptr()));
        }
        rows.sort_by_key(|(id, _)| *id);
        let total = rows.len();
        if total > u16::MAX as usize {
            return Err("too many rows for one param table".into());
        }

        // Layout: header, row locators, row data, type string, then the 16-aligned
        // wrapper row locators (id -> index) used by the engine's id lookup.
        let type_str = std::ffi::CStr::from_ptr(old.add(type_off as usize) as *const i8).to_bytes_with_nul();
        let data_start = HEADER_SIZE + total * ROW_LOCATOR_SIZE;
        let type_start = data_start + total * ROW_SIZE;
        let wrapper_locs_start = (type_start + type_str.len() + 0xF) & !0xF;
        let file_size = wrapper_locs_start + total * WRAPPER_ROW_LOC_SIZE;

        let alloc = alloc_like(old, WRAPPER_HEADER + file_size)?;
        wr(alloc, wrapper_locs_start as u32);
        wr(alloc.add(4), total as i32);
        let new = alloc.add(WRAPPER_HEADER);

        std::ptr::copy_nonoverlapping(old, new, HEADER_SIZE);
        wr(new, type_start as u32);
        wr(new.add(0x04), data_start as u16);
        wr(new.add(0x0A), total as u16);
        wr(new.add(0x10), type_start as u64);
        wr(new.add(0x30), data_start as u64);
        std::ptr::copy_nonoverlapping(type_str.as_ptr(), new.add(type_start), type_str.len());

        let end_marker = (type_start + type_str.len()) as u64;
        for (i, (id, data)) in rows.iter().enumerate() {
            let offset = data_start + i * ROW_SIZE;
            let loc = new.add(HEADER_SIZE + i * ROW_LOCATOR_SIZE);
            wr(loc, *id as u64);
            wr(loc.add(8), offset as u64);
            wr(loc.add(16), end_marker);
            std::ptr::copy_nonoverlapping(*data, new.add(offset), ROW_SIZE);
            let wloc = new.add(wrapper_locs_start + i * WRAPPER_ROW_LOC_SIZE);
            wr(wloc, *id);
            wr(wloc.add(4), i as i32);
        }

        // Switch the param over to the enlarged copy. The original buffer is left alone.
        *file_ptr_ref = new;
        *file_size_ref = file_size as i64;
        log!("WorldMapPointParam now has {} rows", total);

        // Confirm a minute later that the game is still using our table.
        let (slot, ours_ptr) = (file_ptr_ref as usize, new as usize);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(60));
            let still_ours = *(slot as *const usize) == ours_ptr;
            log!("After 1 min: {}", if still_ours { "markers still in place" } else { "the marker table was REPLACED by the game or another mod" });
        });
        let index: std::collections::HashMap<i32, usize> =
            rows.iter().enumerate().map(|(i, (id, _))| (*id, i)).collect();
        Ok(ids.iter().map(|id| id.map(|id| new.add(data_start + index[&id] * ROW_SIZE) as usize)).collect())
    }
}
