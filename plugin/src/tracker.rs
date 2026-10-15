//! The class notes as progress trackers. Every note's description is rebuilt in one layout from
//! generated::NOTE_TEXTS (Convergence's own entries, see tools/generate.py note_texts):
//!
//! ```text
//! [icon] Nox Oracle   7 / 15 found                      (class colour)
//! The Nox Oracle primarily uses ...
//!
//! Runes of Night   2 / 5                               (tan)
//! Shimmering Rune of Night - Raya Lucaria Crystal Tunnel (name in class colour)
//! Glowing Rune of Night - Mirage Rise                    (grey once found)
//! ```
//!
//! An entry is found once all its items are obtained (classes.rs rule). The text lives in our copy
//! of the description text (msg.rs), with room for its longest form; when your progress changes it
//! is rewritten in place there, so nothing is re-allocated while playing.

use crate::generated::{NOTES, NOTE_TEXTS};
use crate::markers::Item;
use crate::msg::{fmg_string, in_colour, note_img, ICON_PX};
use crate::notetext;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// A class note, as the plugin writes it.
pub struct NoteText {
    pub note: u32,
    /// The note whose "The <class> primarily uses ..." line this one shows (notetext.rs).
    pub intro_note: u32,
    pub sections: &'static [NoteSection],
}

pub struct NoteSection {
    /// 1 runes ("Runes of Night"), 3 armor, 2 weapons.
    pub kind: u8,
    /// The note whose lines the entries are.
    pub source: u32,
    pub entries: &'static [NoteEntry],
}

/// One line of a note, as Convergence wrote it.
pub struct NoteEntry {
    /// Its line in the source note, and the one item to name it after (notetext.rs).
    pub line: u16,
    pub name_item: Option<(u8, u32)>,
    pub items: &'static [Item],
    /// Starts a new group (an armor set): a blank line before it.
    pub new_group: bool,
}

/// Section headings.
pub const HEADING: &str = "#C0B194";
/// What you've found.
const FOUND: &str = "#8A8378";
/// Item names of a class without a colour (Convergence's own).
const DEFAULT_NAME: &str = "#FFA500";
/// Text size below the title. The description box draws 18-22 at one smaller size (14 is too
/// small to read); the default is about 24.
const BODY_SIZE: u32 = 20;
/// Room for the longest form of a note, beyond the longer of "nothing found" and "all found".
const SPARE: usize = 64;

/// Where a note's text lives: the text file slot, our copy of it there (patch_and_watch keeps it
/// current), and the notes in it.
struct Place {
    slot: usize,
    ours: Arc<AtomicUsize>,
    notes: Vec<u32>,
    /// By note: the copy we last wrote its text into, and that text's version.
    written: HashMap<u32, (usize, u32)>,
}

/// A note's current text, its room (UTF-16 units) and version (counts its rebuilds).
struct Text {
    units: Vec<u16>,
    room: usize,
    version: u32,
}

struct State {
    colours: HashMap<&'static str, String>,
    texts: HashMap<u32, Text>,
    /// Which entries were found when the text was last built, by note (none yet = not built).
    found: HashMap<u32, Vec<bool>>,
    places: Vec<Place>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn name(s: &NoteSection, e: &NoteEntry) -> String {
    notetext::name(s.source, e.line, e.name_item)
}

fn grey(s: &str) -> String {
    format!("<font color=\"{}\">{}</font>", FOUND, s)
}

/// The note's text with `found` deciding each entry.
fn build(t: &NoteText, colours: &HashMap<&'static str, String>, found: &dyn Fn(&NoteEntry) -> bool) -> String {
    let Some(note) = NOTES.iter().find(|n| n.goods_id == t.note) else { return String::new() };
    let class = note.class;
    let name_colour = colours.get(class).map(String::as_str).unwrap_or(DEFAULT_NAME);
    let total: usize = t.sections.iter().map(|s| s.entries.len()).sum();
    let got: usize = t.sections.iter().flat_map(|s| s.entries).filter(|e| found(e)).count();
    let title = format!(
        "{}{}",
        note_img(note, ICON_PX),
        in_colour(&format!("{}   {} / {} found", class, got, total), class, colours)
    );
    let mut out = notetext::intro(t.intro_note, class);
    for s in t.sections {
        let got = s.entries.iter().filter(|e| found(e)).count();
        let heading = match s.kind {
            1 => s.entries.first().map(|e| notetext::rune_heading(&name(s, e))).unwrap_or_default(),
            2 => "Weapons".to_string(),
            _ => "Armor".to_string(),
        };
        out += &format!("\n\n<font color=\"{}\">{}   {} / {}</font>", HEADING, heading, got, s.entries.len());
        for e in s.entries {
            if e.new_group {
                out += "\n";
            }
            out += "\n";
            out += &if found(e) {
                grey(&format!("{} - {}", name(s, e), notetext::location(s.source, e.line)))
            } else {
                format!(
                    "<font color=\"{}\">{}</font> - {}",
                    name_colour,
                    name(s, e),
                    notetext::location(s.source, e.line)
                )
            };
        }
    }
    // The title at the normal size, everything below it smaller.
    format!("{}\n<font size=\"{}\">{}</font>", title, BODY_SIZE, out)
}

/// Prepare every note's text (nothing found yet) and its room. Call before the text is patched.
pub fn init(colours: HashMap<&'static str, String>) {
    let mut texts = HashMap::new();
    for t in NOTE_TEXTS {
        let none: Vec<u16> = build(t, &colours, &|_| false).encode_utf16().collect();
        let all = build(t, &colours, &|_| true).encode_utf16().count();
        let room = none.len().max(all) + SPARE;
        texts.insert(t.note, Text { units: none, room, version: 0 });
    }
    let _ = STATE.set(Mutex::new(State { colours, texts, found: HashMap::new(), places: Vec::new() }));
}

/// Whether `id` is a note we rewrite.
pub fn is_note(id: i32) -> bool {
    STATE.get().is_some_and(|s| s.lock().unwrap().texts.contains_key(&(id as u32)))
}

/// A note's current text, padded with terminators to its room (what our text file copy holds).
pub fn padded(id: i32) -> Option<Vec<u16>> {
    let state = STATE.get()?.lock().unwrap();
    let text = state.texts.get(&(id as u32))?;
    let mut out = text.units.clone();
    out.resize(text.room, 0);
    Some(out)
}

/// Our copy of text file `slot`, kept current in `ours`, holds `notes`.
pub fn register(slot: *mut *mut u8, ours: Arc<AtomicUsize>, notes: Vec<u32>) {
    if let Some(s) = STATE.get() {
        s.lock().unwrap().places.push(Place { slot: slot as usize, ours, notes, written: HashMap::new() });
    }
}

/// Rebuild the notes whose progress changed, and write every note into each copy of ours that
/// doesn't hold its current text yet: a copy made (or remade after the game reloaded its text)
/// while a rebuild happened can hold an older one. `obtained`: the classes.rs rule for one item.
pub fn update(obtained: &dyn Fn(&Item) -> bool) {
    let Some(state) = STATE.get() else { return };
    let mut state = state.lock().unwrap();
    let State { colours, texts, found: built, places } = &mut *state;
    let found = |e: &NoteEntry| !e.items.is_empty() && e.items.iter().all(obtained);
    for t in NOTE_TEXTS {
        // Only rebuild a note's text when one of its entries was found or lost.
        let now: Vec<bool> = t.sections.iter().flat_map(|s| s.entries).map(found).collect();
        if built.get(&t.note) == Some(&now) {
            continue;
        }
        built.insert(t.note, now);
        let units: Vec<u16> = build(t, colours, &found).encode_utf16().collect();
        let Some(text) = texts.get_mut(&t.note) else { continue };
        if units.len() >= text.room {
            crate::log!("Class notes: note {}'s text outgrew its room; not updated", t.note);
            continue;
        }
        if text.units != units {
            text.units = units;
            text.version += 1;
        }
    }
    for p in places.iter_mut() {
        // Only into our own copy: right after the game reloads its text, the slot briefly holds
        // the game's (patch_and_watch puts ours back, and the next update writes into it).
        let fmg = unsafe { *(p.slot as *const usize) };
        if fmg == 0 || fmg != p.ours.load(Ordering::Relaxed) {
            continue;
        }
        for &note in &p.notes {
            let Some(text) = texts.get(&note) else { continue };
            if p.written.get(&note) == Some(&(fmg, text.version)) {
                continue;
            }
            if let Some(at) = unsafe { fmg_string(fmg as *const u8, note as i32) } {
                unsafe {
                    std::ptr::copy_nonoverlapping(text.units.as_ptr(), at, text.units.len());
                    std::ptr::write_bytes(at.add(text.units.len()), 0, text.room - text.units.len());
                }
            }
            p.written.insert(note, (fmg, text.version));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes every note (nothing found) to CM_NOTES_OUT.
    #[test]
    #[ignore = "needs real game files, see the CM_* variables above"]
    fn write_all_notes() {
        let Ok(out) = std::env::var("CM_NOTES_OUT") else { return };
        crate::notetext::tests::load_from_export();
        let colours = HashMap::new();
        let mut all = String::new();
        for t in NOTE_TEXTS {
            let name = NOTES.iter().find(|n| n.goods_id == t.note).map(|n| format!("{} ({})", n.class, n.which));
            all += &format!(
                "===== {} {}
{}

",
                t.note,
                name.unwrap_or_default(),
                super::build(t, &colours, &|_| false)
            );
        }
        std::fs::write(out, all).unwrap();
    }

    /// Prints two notes with every other entry found.
    #[test]
    fn print_notes() {
        let colours: HashMap<&'static str, String> = [("Nox Oracle", "#667DE4".to_string())].into();
        let mut k = 0;
        let found = |_: &NoteEntry| {
            k += 1;
            k % 2 == 0
        };
        let found = std::cell::RefCell::new(found);
        for t in NOTE_TEXTS.iter().filter(|t| t.note == 8812 || t.note == 8800) {
            println!("----- {}\n{}\n", t.note, super::build(t, &colours, &|e| (found.borrow_mut())(e)));
        }
    }
}
