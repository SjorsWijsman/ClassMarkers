//! Text the game shows, extended in memory:
//! - marker labels: our own entries in the PlaceName text;
//! - item descriptions: a line naming the classes (note icon + name in the class colour) at the
//!   start of every item a class note lists.
//!
//! A loaded text file (FMG) is rebuilt into a larger buffer with our entries and its
//! MsgRepository slot is pointed at the copy. Same approach as Map For Goblins - DLL
//! (goblin_messages.cpp: patch_fmg_in_memory), including allocating from the engine allocator
//! that owns the original buffer so the engine can free ours. A watcher re-applies a patch if the
//! game ever loads that text again.
//!
//! Each text family exists in three layers, each in its own slot (its file id in item.msgbnd):
//! base, _dlc01 and _dlc02. The game looks an id up in the highest layer first, so a description
//! is changed in the highest layer that has it.

use crate::generated::NOTES;
use crate::markers::Note;
use crate::inject::alloc_like;
use crate::log;
use crate::noteicons::NAME_PREFIX;
use crate::scan;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// `mov rdi, [rip+X]` loading the MsgRepositoryImp singleton.
const MSG_REPOSITORY_AOB: &str = "48 8B 3D ?? ?? ?? ?? 44 0F B6 30 48 85 FF 75";
/// PlaceName's slot (its file id in item.msgbnd).
const PLACE_NAME_SLOT: usize = 19;
/// Description slots per ItemLot category, lowest layer first: base, _dlc01, _dlc02.
const CAPTION_SLOTS: [(u8, [usize; 3]); 4] = [
    (1, [24, 321, 421]), // GoodsCaption
    (2, [25, 312, 412]), // WeaponCaption
    (3, [26, 315, 415]), // ProtectorCaption
    (4, [27, 318, 418]), // AccessoryCaption
];
/// On-screen size of a note's own (square) icon in item descriptions, in pixels. A text box puts
/// an image's bottom on the text baseline and grows the line upward for anything taller (vspace is
/// ignored), so the icon files are cut to sit on the baseline like a letter (tools/note_icons.py).
pub const ICON_PX: u32 = 32;
/// The same on the map markers' text, which is smaller: an icon taller than its line pushes the
/// line down.
pub const MAP_ICON_PX: u32 = 22;
/// Our icon files are drawn at `icon_px` per this many image pixels, in both directions, so a
/// file's own width and height count (the exported icons are this size).
const ICON_FILE_PX: u32 = 96;

/// A note's icon as text markup, `icon_px` per ICON_FILE_PX: ours from icons/notes if there is one
/// (noteicons.rs), else the note's own item icon. Used in item descriptions and the map markers'
/// text. Text boxes understand simple HTML: <font color>, <u>, and <img> of atlas entries work;
/// bold, italic, other fonts and alignment don't.
pub fn note_img(note: &Note, icon_px: u32) -> String {
    let scale = |px: u32| ((icon_px * px + ICON_FILE_PX / 2) / ICON_FILE_PX).max(1);
    let (name, width, height) = match crate::NOTE_ICONS.get().and_then(|o| o.get(&note.goods_id)) {
        Some(&(w, h)) => (format!("{}{}", NAME_PREFIX, note.goods_id), scale(w), scale(h)),
        None => (format!("MENU_ItemIcon_{:05}", note.icon), icon_px, icon_px),
    };
    format!("<img src=\"img://{}\" width=\"{}\" height=\"{}\">", name, width, height)
}

/// `text` in the class's colour (`colours`: class -> "#RRGGBB", see colors.rs), if it has one.
pub fn in_colour(text: &str, class: &str, colours: &HashMap<&str, String>) -> String {
    match colours.get(class) {
        Some(c) => format!("<font color=\"{}\">{}</font>", c, text),
        None => text.to_string(),
    }
}

/// The line added to an item's description: each class with its note's icon, its name in the
/// class's colour.
fn class_line(notes: &[u32], colours: &HashMap<&str, String>) -> String {
    let classes: Vec<String> = notes
        .iter()
        .filter_map(|id| NOTES.iter().find(|n| n.goods_id == *id))
        .map(|note| format!("{}{}", note_img(note, ICON_PX), in_colour(note.class, note.class, colours)))
        .collect();
    classes.join(", ")
}

/// Our label ids: far above anything the game or Convergence uses.
pub const FIRST_TEXT_ID: i32 = 1_850_000_000;

unsafe fn rd<T: Copy>(p: *const u8) -> T {
    std::ptr::read_unaligned(p as *const T)
}
unsafe fn wr<T>(p: *mut u8, v: T) {
    std::ptr::write_unaligned(p as *mut T, v)
}

/// The MsgRepository's slot array and its length, once the game has loaded its text.
fn repository() -> Result<(*mut *mut u8, usize), String> {
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
                        return Ok((sub, count as usize));
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err("the game's text was not loaded after 5 minutes".into())
}

/// Slot `index` of the repository, if it exists and holds a loaded text file.
fn loaded_slot(repo: (*mut *mut u8, usize), index: usize) -> Option<*mut *mut u8> {
    let (sub, count) = repo;
    if index >= count {
        return None;
    }
    let slot = unsafe { sub.add(index) };
    let fmg = unsafe { *slot };
    // A plausible FMG header: version 2.
    (!fmg.is_null() && unsafe { rd::<u32>(fmg) } == 0x0002_0000).then_some(slot)
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

/// Where string `id` of an in-memory FMG is, if it has one.
pub unsafe fn fmg_string(fmg: *const u8, id: i32) -> Option<*mut u16> {
    let groups: u32 = rd(fmg.add(0x0C));
    let raw: u64 = rd(fmg.add(0x18));
    let offsets = if raw > 0x100_0000 { raw as *const u8 } else { fmg.add(raw as usize) };
    for g in 0..groups as usize {
        let grp = fmg.add(0x28 + g * 16);
        let (index, first, last): (i32, i32, i32) = (rd(grp), rd(grp.add(4)), rd(grp.add(8)));
        if (first..=last).contains(&id) {
            let off: u64 = rd(offsets.add((index + id - first) as usize * 8));
            if off == 0 {
                return None;
            }
            let p = if off > 0x100_0000 { off as *mut u16 } else { fmg.add(off as usize) as *mut u16 };
            return Some(p);
        }
    }
    None
}

/// Our entries for a loaded text file, computed from that file (so a re-load is patched afresh).
type Entries = Box<dyn Fn(*const u8) -> Vec<(i32, Vec<u16>)> + Send>;

/// Point `slot` at a copy of its text file with `entries` added, and keep it that way: a watcher
/// re-applies the patch if the game ever loads that text again (and calls `on_lost` if it can't).
/// Returns where our current copy is (updated by the watcher).
fn patch_and_watch(
    slot: *mut *mut u8,
    what: &'static str,
    entries: Entries,
    on_lost: impl Fn() + Send + 'static,
) -> Result<Arc<AtomicUsize>, String> {
    let patched = unsafe { build_fmg(*slot, &entries(*slot))? };
    unsafe { *slot = patched };
    let ours = Arc::new(AtomicUsize::new(patched as usize));
    let shared = ours.clone();
    let (slot, mut current) = (slot as usize, patched as usize);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        let now = unsafe { *(slot as *const usize) };
        if now == current || now == 0 {
            continue;
        }
        log!("The game reloaded its {} text; adding ours again", what);
        match unsafe { build_fmg(now as *const u8, &entries(now as *const u8)) } {
            Ok(p) => {
                shared.store(p as usize, Ordering::Relaxed);
                unsafe { *(slot as *mut usize) = p as usize };
                current = p as usize;
            }
            Err(e) => {
                log!("Could not re-add our {} text ({})", what, e);
                on_lost();
                return;
            }
        }
    });
    Ok(ours)
}

/// Add `labels` to PlaceName as ids FIRST_TEXT_ID, FIRST_TEXT_ID + 1, ...
/// On success the ids can be used as marker text; `on_lost` runs if they are lost later.
pub fn add_labels(labels: Vec<String>, on_lost: impl Fn() + Send + 'static) -> Result<(), String> {
    let ours: Vec<(i32, Vec<u16>)> = labels
        .iter()
        .enumerate()
        .map(|(i, s)| (FIRST_TEXT_ID + i as i32, s.encode_utf16().collect()))
        .collect();
    let repo = repository()?;
    let slot = loaded_slot(repo, PLACE_NAME_SLOT).ok_or("place-name text not loaded")?;
    let count = ours.len();
    patch_and_watch(slot, "place-name", Box::new(move |_| ours.clone()), on_lost)?;
    log!("Added {} labels to the map's place names", count);
    Ok(())
}

/// Start the description of every item in `captions` (ItemLot category, item id, [goods ids of the
/// notes listing it]) with a line naming those classes, and replace the class notes' descriptions
/// with their tracker text (tracker.rs). Returns how many descriptions were changed.
pub fn add_class_lines(captions: &'static [(u8, u32, &'static [u32])]) -> Result<usize, String> {
    let repo = repository()?;
    let colours = log::dll_dir().map(|d| crate::colors::class_colors(d)).unwrap_or_default();
    let classes: std::collections::HashSet<_> = NOTES.iter().map(|n| n.class).collect();
    log!("Class colours: {} of {} classes", colours.len(), classes.len());
    crate::tracker::init(colours.clone());
    let mut done = 0;
    for (category, slots) in CAPTION_SLOTS {
        let mut wanted: HashMap<i32, &[u32]> = captions
            .iter()
            .filter(|(c, _, _)| *c == category)
            .map(|&(_, id, listed)| (id as i32, listed))
            .collect();
        if category == 1 {
            for t in crate::generated::NOTE_TEXTS {
                wanted.entry(t.note as i32).or_insert(&[]);
            }
        }
        if wanted.is_empty() {
            continue;
        }
        // Each id goes to the highest layer that has a description for it.
        let layers: Vec<Option<*mut *mut u8>> = slots.iter().map(|&s| loaded_slot(repo, s)).collect();
        let mut owner: HashMap<i32, usize> = HashMap::new();
        for (l, slot) in layers.iter().enumerate() {
            if let Some(slot) = slot {
                for (id, _) in unsafe { read_fmg(**slot) } {
                    if wanted.contains_key(&id) {
                        owner.insert(id, l);
                    }
                }
            }
        }
        for (l, slot) in layers.iter().enumerate() {
            let Some(slot) = *slot else { continue };
            let mine: HashMap<i32, String> = owner
                .iter()
                .filter(|(&id, &o)| o == l && !crate::tracker::is_note(id))
                .map(|(&id, _)| (id, class_line(wanted[&id], &colours)))
                .collect();
            let notes: Vec<u32> = owner
                .iter()
                .filter(|(&id, &o)| o == l && crate::tracker::is_note(id))
                .map(|(&id, _)| id as u32)
                .collect();
            if mine.is_empty() && notes.is_empty() {
                continue;
            }
            let n = mine.len() + notes.len();
            let entries: Entries = Box::new(move |fmg| {
                unsafe { read_fmg(fmg) }
                    .into_iter()
                    .filter_map(|(id, text)| {
                        // A class note: its tracker text (with room to grow).
                        if let Some(note) = crate::tracker::padded(id) {
                            return Some((id, note));
                        }
                        // Our line first, then the description on the next line.
                        let line = mine.get(&id)?;
                        let mut out: Vec<u16> = format!("{}\n", line).encode_utf16().collect();
                        out.extend(text);
                        Some((id, out))
                    })
                    .collect()
            });
            let ours = patch_and_watch(slot, "item description", entries, || {})?;
            if !notes.is_empty() {
                log!("Class notes: {} rewritten as trackers", notes.len());
                crate::tracker::register(slot, ours, notes);
            }
            done += n;
        }
    }
    Ok(done)
}
