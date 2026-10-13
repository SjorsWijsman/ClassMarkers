//! Text the game shows, extended in memory:
//! - marker labels: our own entries in the PlaceName text;
//! - item descriptions: a line naming the classes (note icon + name in the class colour) at the
//!   start of every item a class note lists;
//! - the class notes' own descriptions, rewritten as progress trackers (tracker.rs).
//!
//! A loaded text file (FMG) is rebuilt into a larger buffer with our entries and its
//! MsgRepository slot is pointed at the copy. Same approach as Map For Goblins - DLL
//! (goblin_messages.cpp: patch_fmg_in_memory), including allocating from the engine allocator
//! that owns the original buffer so the engine can free ours. One watcher thread re-applies a
//! patch if the game ever loads that text again.
//!
//! Each text family exists in three layers, each in its own slot (its file id in item.msgbnd):
//! base, _dlc01 and _dlc02. The game looks an id up in the highest layer first, so a description
//! is changed in the highest layer that has it.

use crate::generated::{CAPTION_SLOTS, MARKERS, NOTES, PLACE_NAME_SLOT};
use crate::inject::alloc_like;
use crate::log;
use crate::markers::Note;
use crate::mem::{rd, wr};
use crate::noteicons::NAME_PREFIX;
use eldenring::cs::MsgRepositoryImp;
use fromsoftware_shared::FromStatic;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

/// On-screen size of a note's own (square) icon in item descriptions, in pixels. A text box puts
/// an image's bottom on the text baseline and grows the line upward for anything taller (vspace is
/// ignored), so the icon files are cut to sit on the baseline like a letter (iconart.rs text_icon).
pub const ICON_PX: u32 = 32;
/// The same on the map markers' text, which is smaller: an icon taller than its line pushes the
/// line down.
pub const MAP_ICON_PX: u32 = 22;
/// Our icon files are drawn at `icon_px` per this many image pixels, in both directions, so a
/// file's own width and height count (the icons we make are this size).
const ICON_FILE_PX: u32 = crate::iconart::TEXT_ICON_PX;

/// A note's icon as text markup, `icon_px` per ICON_FILE_PX: ours if there is one (noteicons.rs),
/// else the note's own item icon. Used in item descriptions and the map markers'
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

/// Where an FMG offset points. The game fixes offsets up into absolute pointers when it loads a
/// text file; ours (write_fmg) keep the string offsets relative to the file.
unsafe fn fmg_at(fmg: *const u8, off: u64) -> *const u8 {
    if off > 0x100_0000 {
        off as *const u8
    } else {
        fmg.add(off as usize)
    }
}

/// The game's MsgRepository (found through its type info by fromsoftware-rs), while it exists:
/// none before the text is loaded and once the game has torn it down.
fn repository_object() -> Option<*const u8> {
    MsgRepositoryImp::instance_ptr().ok().map(|p| p as *const u8)
}

/// The MsgRepository's slot array and its length, once the game has loaded its text.
fn repository() -> Result<(*mut *mut u8, usize), String> {
    for _ in 0..600 {
        unsafe {
            if let Some(repo) = repository_object() {
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
    let offsets = fmg_at(fmg, rd(fmg.add(0x18)));
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
            let p = fmg_at(fmg, off) as *const u16;
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
    let entries = merge_entries(read_fmg(original), ours);
    let size = fmg_size(&entries);
    let f = alloc_like(original, size)?;
    write_fmg(f, &entries, size);
    Ok(f)
}

/// `entries` plus `ours` (ours win on equal ids), sorted by id.
fn merge_entries(mut entries: Vec<(i32, Vec<u16>)>, ours: &[(i32, Vec<u16>)]) -> Vec<(i32, Vec<u16>)> {
    let our_ids: HashSet<i32> = ours.iter().map(|(id, _)| *id).collect();
    entries.retain(|(id, _)| !our_ids.contains(id));
    entries.extend(ours.iter().cloned());
    entries.sort_by_key(|(id, _)| *id);
    entries
}

/// Bytes an FMG with `entries` takes (write_fmg's layout).
fn fmg_size(entries: &[(i32, Vec<u16>)]) -> usize {
    let n = entries.len();
    0x28 + n * 16 + n * 8 + entries.iter().map(|(_, t)| (t.len() + 1) * 2).sum::<usize>()
}

/// Write an FMG holding `entries` (sorted by id) into `f`: `size` zeroed bytes (fmg_size).
unsafe fn write_fmg(f: *mut u8, entries: &[(i32, Vec<u16>)], size: usize) {
    let n = entries.len();
    let groups_at = 0x28;
    let offsets_at = groups_at + n * 16;
    let strings_at = offsets_at + n * 8;
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
}

/// Where string `id` of an in-memory FMG is, if it has one.
pub unsafe fn fmg_string(fmg: *const u8, id: i32) -> Option<*mut u16> {
    let groups: u32 = rd(fmg.add(0x0C));
    let offsets = fmg_at(fmg, rd(fmg.add(0x18)));
    for g in 0..groups as usize {
        let grp = fmg.add(0x28 + g * 16);
        let (index, first, last): (i32, i32, i32) = (rd(grp), rd(grp.add(4)), rd(grp.add(8)));
        if (first..=last).contains(&id) {
            let off: u64 = rd(offsets.add((index + id - first) as usize * 8));
            if off == 0 {
                return None;
            }
            return Some(fmg_at(fmg, off) as *mut u16);
        }
    }
    None
}

/// Our entries for a loaded text file, computed from that file (so a re-load is patched afresh).
type Entries = Box<dyn Fn(*const u8) -> Vec<(i32, Vec<u16>)> + Send>;

/// A patched text file the watcher keeps patched.
struct Watch {
    /// The repository slot, and our copy that should be in it.
    slot: usize,
    ours: Arc<AtomicUsize>,
    what: &'static str,
    entries: Entries,
    on_lost: Box<dyn Fn() + Send>,
}

static WATCHES: Mutex<Vec<Watch>> = Mutex::new(Vec::new());

/// Point `slot` at a copy of its text file with `entries` added, and keep it that way: the watcher
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
    let watch = Watch { slot: slot as usize, ours: ours.clone(), what, entries, on_lost: Box::new(on_lost) };
    WATCHES.lock().unwrap().push(watch);
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(watch_texts);
    });
    Ok(ours)
}

/// Every 5 s, re-apply each patch whose text file the game has loaded again. Stops when the game
/// tears its text down (shutting down), so it never reads freed text files.
fn watch_texts() {
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if repository_object().is_none() {
            return;
        }
        WATCHES.lock().unwrap().retain(|w| {
            let now = unsafe { *(w.slot as *const usize) };
            if now == w.ours.load(Ordering::Relaxed) || now == 0 {
                return true;
            }
            log!("The game reloaded its {} text; adding ours again", w.what);
            match unsafe { build_fmg(now as *const u8, &(w.entries)(now as *const u8)) } {
                Ok(p) => {
                    w.ours.store(p as usize, Ordering::Relaxed);
                    unsafe { *(w.slot as *mut usize) = p as usize };
                    true
                }
                Err(e) => {
                    log!("Could not re-add our {} text ({})", w.what, e);
                    (w.on_lost)();
                    false
                }
            }
        });
    }
}

/// The texts `ids` from a text family's layers (slots, lowest first; a higher layer wins), as
/// the game has them now.
pub fn read_texts(slots: &[usize], ids: &HashSet<i32>) -> Result<HashMap<i32, String>, String> {
    let repo = repository()?;
    let mut out = HashMap::new();
    for &s in slots {
        if let Some(slot) = loaded_slot(repo, s) {
            for (id, text) in unsafe { read_fmg(*slot) } {
                if ids.contains(&id) {
                    out.insert(id, String::from_utf16_lossy(&text));
                }
            }
        }
    }
    Ok(out)
}

/// Add `labels` to PlaceName as ids FIRST_TEXT_ID, FIRST_TEXT_ID + 1, ...
/// On success the ids can be used as marker text; `on_lost` runs if they are lost later.
pub fn add_labels(labels: Vec<String>, on_lost: impl Fn() + Send + 'static) -> Result<(), String> {
    let ours: Vec<(i32, Vec<u16>)> =
        labels.iter().enumerate().map(|(i, s)| (FIRST_TEXT_ID + i as i32, s.encode_utf16().collect())).collect();
    let repo = repository()?;
    let slot = loaded_slot(repo, PLACE_NAME_SLOT).ok_or("place-name text not loaded")?;
    // The place names our markers use must be there, or this slot holds some other text.
    let place_ids: HashSet<i32> = MARKERS.iter().map(|m| m.text_id).filter(|&id| id >= 0).collect();
    let present = unsafe { read_fmg(*slot) }.iter().filter(|(id, _)| place_ids.contains(id)).count();
    if !place_ids.is_empty() && present * 2 < place_ids.len() {
        return Err(format!(
            "slot {} holds only {} of our {} place names, so it isn't PlaceName (unsupported game version?)",
            PLACE_NAME_SLOT,
            present,
            place_ids.len()
        ));
    }
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
    let colours = log::dll_dir().map(|d| crate::colors::class_colors(d)).cloned().unwrap_or_default();
    let classes: std::collections::HashSet<_> = NOTES.iter().map(|n| n.class).collect();
    log!("Class colours: {} of {} classes", colours.len(), classes.len());
    crate::tracker::init(colours.clone());
    let mut done = 0;
    for (category, slots) in CAPTION_SLOTS {
        let mut wanted: HashMap<i32, &[u32]> =
            captions.iter().filter(|(c, _, _)| *c == category).map(|&(_, id, listed)| (id as i32, listed)).collect();
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
        // Nearly every listed item has a description, or these slots hold some other text.
        if owner.len() * 2 < wanted.len() {
            log!(
                "Descriptions for item category {}: only {} of {} found in slots {:?}, skipped (unsupported game version?)",
                category,
                owner.len(),
                wanted.len(),
                slots
            );
            continue;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn fmg_round_trip_keeps_entries_and_ours_win() {
        let original = vec![(5, w("five")), (10, w("ten")), (2, w(""))];
        let ours = vec![(10, w("our ten")), (FIRST_TEXT_ID, w("label"))];
        let entries = merge_entries(original, &ours);
        let size = fmg_size(&entries);
        let mut buf = vec![0u8; size];
        let back = unsafe {
            write_fmg(buf.as_mut_ptr(), &entries, size);
            read_fmg(buf.as_ptr())
        };
        assert_eq!(back, vec![(2, w("")), (5, w("five")), (10, w("our ten")), (FIRST_TEXT_ID, w("label"))]);
        unsafe {
            assert_eq!(rd::<u32>(buf.as_ptr()), 0x0002_0000);
            assert_eq!(rd::<u32>(buf.as_ptr().add(4)) as usize, size);
            assert!(fmg_string(buf.as_ptr(), 7).is_none());
            assert!(fmg_string(buf.as_ptr(), 5).is_some());
        }
    }
}
