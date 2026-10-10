//! Marker definitions and their WORLD_MAP_POINT_PARAM_ST (256-byte) encoding.

pub const ROW_SIZE: usize = 256;

/// An event flag the game keeps permanently on: a marker shows while its eventFlagId is on.
const ALWAYS_ON_FLAG: u32 = 6001;
/// Map icon for every marker: icon 88, the round seal (user's choice; see available_map_icons.png).
const MARKER_ICON: u16 = 88;

/// One generated marker (see tools/generate.py).
pub struct GenMarker {
    /// Goods id of the class's "Note: Path of the <class>"; selects which class it belongs to.
    pub note: u32,
    /// Item category (1 goods, 2 weapon, 3 armor) and id, for item-name labels later.
    #[allow(dead_code)]
    pub kind: u8,
    #[allow(dead_code)]
    pub item_id: i32,
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
    /// PlaceName text shown on hover (-1 = none).
    pub text_id: i32,
    /// Event flag that is set once the item is picked up; the marker hides then (0 = never).
    pub hide_flag: u32,
    /// Map label: the entry's name as written in the class note.
    pub name: &'static str,
    /// For the log.
    pub label: &'static str,
}

/// One text line of a marker.
pub struct Line {
    pub text_id: i32,
    /// Which text list the id refers to: 0 = PlaceName, 1 = NpcName, others unknown yet.
    pub text_type: u8,
    /// Line is hidden once this flag is on (0 = never).
    pub hide_flag: u32,
}

pub struct Row {
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
        put(&mut r, 0x04, &ALWAYS_ON_FLAG.to_le_bytes()); // eventFlagId: shown while on
        put(&mut r, 0x0C, &MARKER_ICON.to_le_bytes()); // iconId
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
        for k in 0..8 {
            let at = 0x30 + 12 * k;
            match self.lines.get(k) {
                Some(l) => {
                    put(&mut r, at, &l.text_id.to_le_bytes()); // textIdN
                    put(&mut r, at + 8, &l.hide_flag.to_le_bytes()); // textDisableFlagIdN
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

impl GenMarker {
    /// The marker's row. `label` = our item-name text id for line 1 (place name on line 2),
    /// or None to show just the place name.
    pub fn row(&self, label: Option<i32>) -> Row {
        let line = |text_id| Line { text_id, text_type: 0, hide_flag: self.hide_flag };
        Row {
            area: self.area,
            grid_x: self.grid_x,
            grid_z: self.grid_z,
            x: self.x,
            y: self.y,
            z: self.z,
            disp_mask: self.disp_mask,
            // The engine hides a marker once all its text lines are hidden, so hiding the
            // label on pickup removes the marker.
            lines: match label {
                Some(id) if self.text_id >= 0 => vec![line(id), line(self.text_id)],
                Some(id) => vec![line(id)],
                None => vec![line(self.text_id)],
            },
        }
    }
}
