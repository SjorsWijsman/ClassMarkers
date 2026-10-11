//! Convergence's words, read from the game's text when it has loaded (never copied into our
//! files): the class notes' entries ("<name> - <where to find it>") and intro lines, and the game's
//! item names. generated.rs refers to entries by note and line number, as tools/generate.py
//! (note_lines) numbers them: the lines holding " - ", in order, formatting removed.

use crate::generated::{MARKERS, NOTES, NOTE_TEXTS};
use crate::log;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Description and name text families: (ItemLot category, slots base / dlc01 / dlc02).
const CAPTIONS: [usize; 3] = [24, 321, 421];
const NAMES: [(u8, [usize; 3]); 4] = [(1, [10, 319, 419]), (2, [11, 310, 410]), (3, [12, 313, 413]), (4, [13, 316, 416])];

struct Texts {
    /// Note goods id -> its entry lines (name, location).
    lines: HashMap<u32, Vec<(String, String)>>,
    /// Note goods id -> its "primarily uses" line.
    intros: HashMap<u32, String>,
    /// (category, item id) -> the game's name for it.
    items: HashMap<(u8, u32), String>,
}

static TEXTS: OnceLock<Texts> = OnceLock::new();

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Read the notes and the item names we need. Call once the game's text is loaded and before we
/// change any of it (msg::add_class_lines rewrites the notes).
pub fn load() -> Result<(), String> {
    load_with(&|slots, ids| crate::msg::read_texts(slots, ids))
}

type Reader<'a> = &'a dyn Fn(&[usize], &HashSet<i32>) -> Result<HashMap<i32, String>, String>;

fn load_with(read: Reader) -> Result<(), String> {
    let note_ids: HashSet<i32> = NOTES.iter().map(|n| n.goods_id as i32).collect();
    let captions = read(&CAPTIONS, &note_ids)?;
    let (mut lines, mut intros) = (HashMap::new(), HashMap::new());
    for (id, text) in captions {
        let text = strip_tags(&text);
        let mut entries = Vec::new();
        for line in text.split('\n').map(str::trim) {
            if let Some((name, loc)) = line.split_once(" - ") {
                entries.push((name.trim().to_string(), loc.trim().to_string()));
            } else if line.contains("primarily uses") || line.contains("favors") {
                intros.entry(id as u32).or_insert_with(|| line.to_string());
            }
        }
        lines.insert(id as u32, entries);
    }
    let wanted: HashSet<(u8, u32)> = MARKERS
        .iter()
        .filter_map(|m| m.name_item)
        .chain(NOTE_TEXTS.iter().flat_map(|t| t.sections).flat_map(|s| s.entries).filter_map(|e| e.name_item))
        .collect();
    let mut items = HashMap::new();
    for (cat, slots) in NAMES {
        let ids: HashSet<i32> = wanted.iter().filter(|(c, _)| *c == cat).map(|&(_, id)| id as i32).collect();
        if !ids.is_empty() {
            for (id, name) in read(&slots, &ids)? {
                items.insert((cat, id as u32), name);
            }
        }
    }
    log!("Read {} class notes and {} item names from the game's text", lines.len(), items.len());
    let _ = TEXTS.set(Texts { lines, intros, items });
    Ok(())
}

fn entry(note: u32, line: u16) -> Option<&'static (String, String)> {
    TEXTS.get()?.lines.get(&note)?.get(line as usize)
}

/// An entry's name: the game's name for its item when it is one item (`item`), else the note's.
pub fn name(note: u32, line: u16, item: Option<(u8, u32)>) -> String {
    item.and_then(|i| TEXTS.get()?.items.get(&i).cloned())
        .or_else(|| entry(note, line).map(|e| e.0.clone()))
        .unwrap_or_default()
}

/// Where the note says to find an entry.
pub fn location(note: u32, line: u16) -> String {
    entry(note, line).map(|e| e.1.clone()).unwrap_or_default()
}

/// A class's "primarily uses" line, from `note` ("Marksman primarily uses" gets its "The").
pub fn intro(note: u32, class: &str) -> String {
    let line = TEXTS.get().and_then(|t| t.intros.get(&note)).cloned().unwrap_or_default();
    if line.starts_with(class) {
        format!("The {}", line)
    } else {
        line
    }
}

/// A rune section's heading from a rune's name: "Runes of Night", "Glintstone Runes".
pub fn rune_heading(rune: &str) -> String {
    let family = rune.split_once(' ').map(|(_, f)| f).unwrap_or(rune);
    if let Some(rest) = family.strip_prefix("Rune of ") {
        format!("Runes of {}", rest)
    } else if family.ends_with("Rune") {
        format!("{}s", family)
    } else {
        family.to_string()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Loads the texts from tab-separated exports of the game's text (slot, id, text with a
    /// backslash-n for each line break): CM_TEXTS=<file>. Returns whether it did.
    pub fn load_from_export() -> bool {
        let Ok(path) = std::env::var("CM_TEXTS") else { return false };
        let mut by_slot: HashMap<usize, HashMap<i32, String>> = HashMap::new();
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let mut f = line.splitn(3, '\t');
            let (slot, id, text) = (f.next().unwrap(), f.next().unwrap(), f.next().unwrap_or(""));
            by_slot.entry(slot.parse().unwrap()).or_default().insert(id.parse().unwrap(), text.replace("\\n", "\n"));
        }
        load_with(&|slots, ids| {
            let mut out = HashMap::new();
            for s in slots {
                for (id, t) in by_slot.get(s).into_iter().flatten() {
                    if ids.contains(id) {
                        out.insert(*id, t.clone());
                    }
                }
            }
            Ok(out)
        })
        .unwrap();
        true
    }
}
