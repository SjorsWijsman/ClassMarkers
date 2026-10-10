//! Marker labels: add our own entries to the game's PlaceName text (FMG) in memory.
//!
//! The PlaceName FMG the game loaded is rebuilt into a larger buffer with our labels added and
//! its MsgRepository slot is pointed at the copy. Same approach as Map For Goblins - DLL
//! (goblin_messages.cpp: patch_fmg_in_memory), including allocating from the engine allocator
//! that owns the original buffer so the engine can free ours.

use crate::inject::alloc_like;
use crate::log;
use crate::scan;
use std::time::Duration;

/// `mov rdi, [rip+X]` loading the MsgRepositoryImp singleton.
const MSG_REPOSITORY_AOB: &str = "48 8B 3D ?? ?? ?? ?? 44 0F B6 30 48 85 FF 75";
/// PlaceName's index among the MsgRepository's text files (its id in item.msgbnd).
const PLACE_NAME_SLOT: usize = 19;
/// Our label ids: far above anything the game or Convergence uses.
pub const FIRST_TEXT_ID: i32 = 1_850_000_000;

unsafe fn rd<T: Copy>(p: *const u8) -> T {
    std::ptr::read_unaligned(p as *const T)
}
unsafe fn wr<T>(p: *mut u8, v: T) {
    std::ptr::write_unaligned(p as *mut T, v)
}

/// The MsgRepository slot holding the PlaceName FMG pointer, once the game has loaded it.
fn place_name_slot() -> Result<*mut *mut u8, String> {
    let hit = *scan::find_all(MSG_REPOSITORY_AOB).first().ok_or("text repository pattern not found")?;
    let repo_slot = unsafe { scan::rip_relative(hit, 3, 7) } as *const *const u8;
    for _ in 0..600 {
        unsafe {
            let repo = *repo_slot;
            if !repo.is_null() {
                let files: *const *mut *mut u8 = rd(repo.add(0x08));
                let count: i32 = rd(repo.add(0x14));
                if !files.is_null() && count > PLACE_NAME_SLOT as i32 {
                    let sub = *files;
                    if !sub.is_null() && !(*sub.add(PLACE_NAME_SLOT)).is_null() {
                        return Ok(sub.add(PLACE_NAME_SLOT));
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err("the game's text was not loaded after 5 minutes".into())
}

/// Every (id, text) in an in-memory FMG (version 2 layout, as loaded by the game).
unsafe fn read_fmg(fmg: *const u8) -> Vec<(i32, Vec<u16>)> {
    let groups: u32 = rd(fmg.add(0x0C));
    let strings: u32 = rd(fmg.add(0x10));
    let raw: u64 = rd(fmg.add(0x18));
    // The game fixes the offsets-table offset up into an absolute pointer when it loads.
    let offsets = if raw > 0x100_0000 { raw as *const u8 } else { fmg.add(raw as usize) };
    let mut out = vec![];
    for g in 0..groups as usize {
        let grp = fmg.add(0x28 + g * 16);
        let (index, first, last): (i32, i32, i32) = (rd(grp), rd(grp.add(4)), rd(grp.add(8)));
        for id in first..=last {
            let si = index + (id - first);
            if si < 0 || si >= strings as i32 {
                continue;
            }
            let off: u64 = rd(offsets.add(si as usize * 8));
            if off == 0 {
                continue;
            }
            let p = if off > 0x100_0000 { off as *const u16 } else { fmg.add(off as usize) as *const u16 };
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            out.push((id, std::slice::from_raw_parts(p, len).to_vec()));
        }
    }
    out
}

/// Build a new FMG holding `original`'s entries plus `ours` (ours win on equal ids).
unsafe fn build_fmg(original: *const u8, ours: &[(i32, Vec<u16>)]) -> Result<*mut u8, String> {
    let mut entries = read_fmg(original);
    let our_ids: std::collections::HashSet<i32> = ours.iter().map(|(id, _)| *id).collect();
    entries.retain(|(id, _)| !our_ids.contains(id));
    entries.extend(ours.iter().cloned());
    entries.sort_by_key(|(id, _)| *id);

    let n = entries.len();
    let groups_at = 0x28;
    let offsets_at = groups_at + n * 16;
    let strings_at = offsets_at + n * 8;
    let size = strings_at + entries.iter().map(|(_, t)| (t.len() + 1) * 2).sum::<usize>();

    let f = alloc_like(original, size)?;
    wr(f, 0x0002_0000u32); // version 2
    wr(f.add(0x04), size as u32);
    wr(f.add(0x08), 1u32);
    wr(f.add(0x0C), n as u32); // one group per entry
    wr(f.add(0x10), n as u32);
    wr(f.add(0x14), 0xFFu32);
    wr(f.add(0x18), f.add(offsets_at) as u64); // already fixed up, like the game's
    let mut at = strings_at;
    for (i, (id, text)) in entries.iter().enumerate() {
        let grp = f.add(groups_at + i * 16);
        wr(grp, i as i32);
        wr(grp.add(4), *id);
        wr(grp.add(8), *id);
        wr(f.add(offsets_at + i * 8), at as u64);
        std::ptr::copy_nonoverlapping(text.as_ptr() as *const u8, f.add(at), text.len() * 2);
        at += (text.len() + 1) * 2; // the buffer is zeroed, so the terminator is already there
    }
    Ok(f)
}

/// Add `labels` to PlaceName as ids FIRST_TEXT_ID, FIRST_TEXT_ID + 1, ...
/// On success the ids can be used as marker text; a watcher re-applies them if the game ever
/// replaces the PlaceName text (and calls `on_lost` if it can't).
pub fn add_labels(labels: Vec<String>, on_lost: impl Fn() + Send + 'static) -> Result<(), String> {
    let ours: Vec<(i32, Vec<u16>)> = labels
        .iter()
        .enumerate()
        .map(|(i, s)| (FIRST_TEXT_ID + i as i32, s.encode_utf16().collect()))
        .collect();
    let slot = place_name_slot()?;
    let patched = unsafe { build_fmg(*slot, &ours)? };
    unsafe { *slot = patched };
    log!("Added {} labels to the map's place names", ours.len());

    let (slot, mut current) = (slot as usize, patched as usize);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        let now = unsafe { *(slot as *const usize) };
        if now == current || now == 0 {
            continue;
        }
        log!("The game replaced its place-name text; adding our labels again");
        match unsafe { build_fmg(now as *const u8, &ours) } {
            Ok(p) => {
                unsafe { *(slot as *mut usize) = p as usize };
                current = p as usize;
            }
            Err(e) => {
                log!("Could not re-add labels ({}); markers fall back to place names", e);
                on_lost();
                return;
            }
        }
    });
    Ok(())
}
