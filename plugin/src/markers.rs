//! Marker definitions and their WORLD_MAP_POINT_PARAM_ST (256-byte) encoding.

use crate::gfx::MapIcons;
use std::collections::HashMap;

pub const ROW_SIZE: usize = 256;

/// Event flags the game keeps permanently on / off. A marker exists on the map while its
/// eventFlagId is on, so switching between these shows or hides it.
pub const ALWAYS_ON_FLAG: u32 = 6001;
pub const ALWAYS_OFF_FLAG: u32 = 6000;
/// Offset of eventFlagId in a WORLD_MAP_POINT_PARAM_ST row.
pub const EVENT_FLAG_OFFSET: usize = 0x04;
/// A row's text lines: textIdN at TEXT_ID_OFFSET + TEXT_LINE_STRIDE * N (textTypeN at 0x90 + N).
const TEXT_ID_OFFSET: usize = 0x30;
const TEXT_LINE_STRIDE: usize = 12;
const TEXT_LINES: usize = 8;
/// Stock map icon for a marker whose note has no icon of ours (gfx.rs): 88, the round seal.
pub const STOCK_ICON: u16 = 88;
/// Longest hint line: the map cuts a marker line off past about 52 characters ("Found after
/// defeating the Godskin Noble on the bridge" still fits after the icon); 40 reads better.
const HINT_CHARS: usize = 40;
/// Start of a wrapped hint line, to line up under the text after the first line's icon.
const HINT_INDENT: &str = "     ";

/// A class note (see tools/generate.py). You own it while it is in your inventory (classes.rs);
/// `flags`, set when one of the item lots that hand out the note is picked up, are the fallback
/// when the inventory can't be read.
pub struct Note {
    pub goods_id: u32,
    pub class: &'static str,
    /// "path" (Note: Path of the <class>) or "weapons" (Note: Weapons for <class>).
    pub which: &'static str,
    pub flags: &'static [u32],
    /// The note's own item icon (EquipParamGoods iconId).
    pub icon: u16,
}

/// An item a marker points to. It counts as obtained once its pickup `flag` is on (0 = none
/// known) or it is in your inventory or storage chest.
pub struct Item {
    /// ItemLot category: 1 = goods, 2 = weapon, 3 = armor, 4 = talisman, 5 = ash of war.
    pub category: u8,
    pub id: u32,
    pub flag: u32,
}

/// `words` filled into lines of at most `width` characters (a longer word gets a line of its own).
fn wrap(words: &[&str], width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for w in words {
        match lines.last_mut() {
            Some(l) if l.chars().count() + 1 + w.chars().count() <= width => {
                l.push(' ');
                l.push_str(w);
            }
            _ => lines.push(w.to_string()),
        }
    }
    lines
}

/// One generated marker (see tools/generate.py).
pub struct GenMarker {
    /// Goods id of the class note that lists this item; the marker shows while you own it.
    pub note: u32,
    /// Item category: 1 = goods (spell runes), 2 = weapon, 3 = armor.
    pub kind: u8,
    /// The note's item(s) found at this spot; the marker hides once all are obtained.
    pub items: &'static [Item],
    /// Map block the row id must encode: AABB for dungeon map mAA_BB, PPGGZZ for world tiles.
    /// The row id is block * 100 + a free two-digit number.
    pub block: i32,
    pub area: u8,
    pub grid_x: u8,
    pub grid_z: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Map layer bits: 1 = overworld, 2 = underground, 4 = Realm of Shadow.
    pub disp_mask: u8,
    /// PlaceName text shown on hover when our labels couldn't be added (-1 = none).
    pub text_id: i32,
    /// Which entry of its note this is: its line (notetext.rs), and the one item to name it after
    /// if it is one item (the game's spelling).
    pub line: u16,
    pub name_item: Option<(u8, u32)>,
    /// For the log.
    pub label: &'static str,
}

/// One text line of a marker.
pub struct Line {
    pub text_id: i32,
    /// Which text list the id refers to: 0 = PlaceName, 1 = NpcName, others unknown yet.
    pub text_type: u8,
}

pub struct Row {
    /// Shown while this flag is on.
    pub event_flag: u32,
    pub icon: u16,
    pub area: u8,
    pub grid_x: u8,
    pub grid_z: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub disp_mask: u8,
    pub lines: Vec<Line>,
}

impl Row {
    pub fn encode(&self) -> [u8; ROW_SIZE] {
        let mut r = [0u8; ROW_SIZE];
        let put = |r: &mut [u8; ROW_SIZE], at: usize, v: &[u8]| r[at..at + v.len()].copy_from_slice(v);
        put(&mut r, EVENT_FLAG_OFFSET, &self.event_flag.to_le_bytes()); // eventFlagId: shown while on
        put(&mut r, 0x0C, &self.icon.to_le_bytes()); // iconId
        if self.lines.iter().all(|l| l.text_id < 0) {
            r[0x10] |= 0b100; // isEnableNoText: show the icon even without a label
        }
        r[0x18] = self.disp_mask; // dispMask00..02
        r[0x20] = self.area;
        r[0x21] = self.grid_x;
        r[0x22] = self.grid_z;
        put(&mut r, 0x24, &self.x.to_le_bytes());
        put(&mut r, 0x28, &self.y.to_le_bytes());
        put(&mut r, 0x2C, &self.z.to_le_bytes());
        for k in 0..TEXT_LINES {
            let at = TEXT_ID_OFFSET + TEXT_LINE_STRIDE * k;
            match self.lines.get(k) {
                Some(l) => {
                    put(&mut r, at, &l.text_id.to_le_bytes()); // textIdN
                    r[0x90 + k] = l.text_type; // textTypeN
                }
                None => put(&mut r, at, &(-1i32).to_le_bytes()),
            }
        }
        for at in [0x98, 0xA8, 0xAC, 0xB0] {
            put(&mut r, at, &(-1i32).to_le_bytes()); // distViewId, distViewId1..3 unused
        }
        r[0xB5] = 1; // selectMinZoomStep
        r
    }
}

/// Blank the text lines of the live row at `row` whose text id is `first_id` or above.
pub unsafe fn clear_texts_from(row: usize, first_id: i32) {
    for k in 0..TEXT_LINES {
        let at = (row + TEXT_ID_OFFSET + TEXT_LINE_STRIDE * k) as *mut u8;
        if crate::mem::rd::<i32>(at) >= first_id {
            crate::mem::wr(at, -1i32);
        }
    }
}

impl GenMarker {
    /// The entry's name (the item's own when it is one item).
    fn name(&self) -> String {
        crate::notetext::name(self.note, self.line, self.name_item)
    }

    /// Where the note says to find it.
    fn hint(&self) -> String {
        crate::notetext::location(self.note, self.line)
    }

    fn note(&self) -> Option<&'static Note> {
        crate::generated::NOTES.iter().find(|n| n.goods_id == self.note)
    }

    /// Map label: "<class> - <name as written in the note>", the class in its colour (`colours`:
    /// class -> "#RRGGBB", see colors.rs), e.g. "Nox Oracle - Shimmering Rune of Night".
    pub fn label_text(&self, colours: &HashMap<&str, String>) -> String {
        match self.note() {
            Some(n) => format!("{} - {}", crate::msg::in_colour(n.class, n.class, colours), self.name()),
            None => self.name(),
        }
    }

    /// The following text lines: where the class note says to find it, after the note's icon (the
    /// same as in item descriptions, smaller). A marker line is cut off when too long, so a hint
    /// is wrapped at word breaks, each line filled up to HINT_CHARS (none if the note gives no
    /// hint).
    pub fn hint_lines(&self) -> Vec<String> {
        let hint = self.hint();
        let words: Vec<&str> = hint.split_whitespace().collect();
        if words.is_empty() {
            return Vec::new();
        }
        let chunks = wrap(&words, HINT_CHARS);
        match self.note() {
            // In the notes' heading colour (tracker.rs), so it reads as the note speaking. The
            // icon starts the first line; the others are indented to line up under its text.
            Some(n) => chunks
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    let lead =
                        if k == 0 { crate::msg::note_img(n, crate::msg::MAP_ICON_PX) } else { HINT_INDENT.to_string() };
                    format!("{}<font color=\"{}\">{}</font>", lead, crate::tracker::HEADING, c)
                })
                .collect(),
            None => chunks
                .iter()
                .enumerate()
                .map(|(k, c)| if k == 0 { format!("Note: {}", c) } else { c.clone() })
                .collect(),
        }
    }

    /// The marker's text: the label, then the hint lines.
    pub fn texts(&self, colours: &HashMap<&str, String>) -> Vec<String> {
        std::iter::once(self.label_text(colours)).chain(self.hint_lines()).collect()
    }

    /// The marker's row: our text ids `texts` (label "<class> - <item>", then the hint lines
    /// "<note icon><where the note says>"), or just the place when there's no text of ours.
    /// `icons`: our icons (see gfx.rs): a spell rune marker gets its rune tier's icon, others (or
    /// without one) the note's icon, else the stock icon.
    pub fn row(&self, texts: Option<&[i32]>, icons: &MapIcons) -> Row {
        let line = |text_id| Line { text_id, text_type: 0 };
        let lines = match texts {
            Some(ids) => ids.iter().take(TEXT_LINES).map(|&id| line(id)).collect(),
            None => vec![line(self.text_id)],
        };
        Row {
            // Hidden until the visibility check (classes.rs) has seen that you own the note and
            // haven't obtained the item(s) yet.
            event_flag: ALWAYS_OFF_FLAG,
            icon: (self.kind == 1)
                .then(|| {
                    let name = self.name();
                    let first = name.split(' ').next().unwrap_or("");
                    crate::gfx::RUNE_TIERS.iter().copied().find(|t| *t == first)
                })
                .flatten()
                .and_then(|tier| icons.get(&(self.note, tier)))
                .or_else(|| icons.get(&(self.note, "")))
                .copied()
                .unwrap_or(STOCK_ICON),
            area: self.area,
            grid_x: self.grid_x,
            grid_z: self.grid_z,
            x: self.x,
            y: self.y,
            z: self.z,
            disp_mask: self.disp_mask,
            lines,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i32_at(r: &[u8], at: usize) -> i32 {
        i32::from_le_bytes(r[at..at + 4].try_into().unwrap())
    }

    fn row(lines: Vec<Line>) -> Row {
        Row {
            event_flag: ALWAYS_ON_FLAG,
            icon: 371,
            area: 60,
            grid_x: 42,
            grid_z: 36,
            x: 1.5,
            y: -2.0,
            z: 3.25,
            disp_mask: 1,
            lines,
        }
    }

    #[test]
    fn encode_puts_fields_at_their_offsets() {
        let r = row(vec![Line { text_id: 1000, text_type: 0 }, Line { text_id: 1_850_000_000, text_type: 1 }]).encode();
        assert_eq!(i32_at(&r, EVENT_FLAG_OFFSET), ALWAYS_ON_FLAG as i32);
        assert_eq!(u16::from_le_bytes([r[0x0C], r[0x0D]]), 371);
        assert_eq!(r[0x10] & 0b100, 0, "has text, so not isEnableNoText");
        assert_eq!((r[0x18], r[0x20], r[0x21], r[0x22]), (1, 60, 42, 36));
        assert_eq!(f32::from_le_bytes(r[0x24..0x28].try_into().unwrap()), 1.5);
        assert_eq!(f32::from_le_bytes(r[0x28..0x2C].try_into().unwrap()), -2.0);
        assert_eq!(f32::from_le_bytes(r[0x2C..0x30].try_into().unwrap()), 3.25);
        assert_eq!((i32_at(&r, 0x30), r[0x90]), (1000, 0));
        assert_eq!((i32_at(&r, 0x3C), r[0x91]), (1_850_000_000, 1));
        for k in 2..8 {
            assert_eq!(i32_at(&r, 0x30 + 12 * k), -1, "unused text line {}", k);
        }
        for at in [0x98, 0xA8, 0xAC, 0xB0] {
            assert_eq!(i32_at(&r, at), -1);
        }
        assert_eq!(r[0xB5], 1);
    }

    #[test]
    fn encode_without_text_shows_icon_only() {
        let r = row(vec![Line { text_id: -1, text_type: 0 }]).encode();
        assert_eq!(r[0x10] & 0b100, 0b100);
    }
}
