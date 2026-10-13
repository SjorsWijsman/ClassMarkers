//! Decide which markers show: those of the class notes you own, for items you haven't obtained.
//!
//! Owning a note = it is in your inventory (not the storage chest), read through the game's own
//! data (GameDataMan) on the game's thread. So you choose which classes to track: move a note to
//! the chest (or drop it) to hide its markers, take it back out to show them again. If the
//! inventory can't be read, owning falls back to "any of the note's pickup flags is on" (see
//! generated::NOTES).
//!
//! An item is obtained once you picked it up at the marked spot (its pickup flag is on), or it is
//! in your inventory or storage chest (bought, found elsewhere, ...). Selling or dropping it later
//! doesn't bring the marker back: the pickup flag stays on. A marker for several items (an armor
//! set) hides once all of them are obtained.
//!
//! Our rows' eventFlagId is set to the always-on or always-off flag accordingly. The map reads it
//! when it is opened, so changes show the next time you open the map.

use crate::flags::Flags;
use crate::generated::NOTES;
use crate::log;
use crate::markers::{Item, ALWAYS_OFF_FLAG, ALWAYS_ON_FLAG, EVENT_FLAG_OFFSET};
use eldenring::cs::{CSTaskGroupIndex, CSTaskImp, GameDataMan, ItemCategory};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::{FromStatic, SharedTaskImpExt};
use std::collections::{BTreeSet, HashSet};
use std::time::Duration;

/// Re-check every this many frames (about twice a second at 60 fps).
const CHECK_EVERY_FRAMES: u32 = 30;

/// One marker row to keep up to date.
pub struct Watched {
    /// Goods id of the class note that lists the marker's item(s).
    pub note: u32,
    pub items: &'static [Item],
    /// Address of the row's data in the live table.
    pub row: usize,
}

/// What you have, as (ItemLot category, id): in your inventory, and in inventory or storage chest.
struct Owned {
    carried: HashSet<(u8, u32)>,
    items: HashSet<(u8, u32)>,
}

impl Owned {
    /// None if the main player's inventory can't be read (no character loaded yet).
    fn read() -> Option<Owned> {
        let game_data = unsafe { GameDataMan::instance() }.ok()?;
        let player = &game_data.main_player_game_data;
        let add = |items: &mut HashSet<(u8, u32)>, inventory: &eldenring::cs::InventoryItemsData| {
            for e in inventory.items() {
                let id = e.item_id.param_id();
                let category = match e.item_id.category() {
                    ItemCategory::Goods => 1,
                    ItemCategory::Weapon => {
                        for w in weapon_ids(id) {
                            items.insert((2, w));
                        }
                        continue;
                    }
                    ItemCategory::Protector => 3,
                    ItemCategory::Accessory => 4,
                    ItemCategory::Gem => 5,
                };
                items.insert((category, id));
            }
        };
        let mut carried = HashSet::new();
        add(&mut carried, &player.equipment.equip_inventory_data.items_data);
        let mut items = carried.clone();
        if let Some(storage) = &player.storage {
            add(&mut items, &storage.items_data);
        }
        Some(Owned { carried, items })
    }

    fn has(&self, category: u8, id: u32) -> bool {
        self.items.contains(&(category, id))
    }

    /// The class notes in your inventory: a note in the storage chest doesn't count.
    fn notes(&self) -> BTreeSet<u32> {
        NOTES.iter().map(|n| n.goods_id).filter(|&id| self.carried.contains(&(1, id))).collect()
    }
}

/// The weapon ids an owned weapon counts as. A weapon id is base + affinity * 100 + upgrade level:
/// an upgraded copy counts as the weapon with that affinity, and as the plain weapon.
fn weapon_ids(id: u32) -> [u32; 2] {
    [id - id % 100, id - id % 10000]
}

/// The class notes whose pickup flags are on (fallback).
fn notes_from_flags(flags: &Flags) -> BTreeSet<u32> {
    NOTES.iter().filter(|n| n.flags.iter().any(|&f| flags.get(f) == Some(true))).map(|n| n.goods_id).collect()
}

fn obtained(item: &Item, owned: Option<&Owned>, flags: Option<&Flags>) -> bool {
    let picked_up = item.flag > 0 && flags.and_then(|f| f.get(item.flag)) == Some(true);
    picked_up || owned.is_some_and(|o| o.has(item.category, item.id))
}

fn describe(notes: &BTreeSet<u32>) -> String {
    let names: Vec<String> =
        NOTES.iter().filter(|n| notes.contains(&n.goods_id)).map(|n| format!("{} ({})", n.class, n.which)).collect();
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
    }
}

pub fn watch(rows: Vec<Watched>) {
    let flags = Flags::resolve()
        .map_err(|e| log!("Event flags can't be read ({}); only items you own count as obtained", e))
        .ok();
    let task = match CSTaskImp::wait_for_instance(Duration::from_secs(300)) {
        Ok(t) => t,
        Err(_) => {
            log!("Class notes: the game's task system was not found; markers stay hidden");
            return;
        }
    };
    let mut frame = 0u32;
    let mut last: Option<(BTreeSet<u32>, Vec<bool>)> = None;
    let mut source_logged = None;
    let handle = task.run_recurring(
        move |_: &FD4TaskData| {
            frame = frame.wrapping_add(1);
            if !frame.is_multiple_of(CHECK_EVERY_FRAMES) {
                return;
            }
            let owned = Owned::read();
            let (notes, source) = match (&owned, &flags) {
                (Some(o), _) => (o.notes(), "inventory"),
                (None, Some(f)) => (notes_from_flags(f), "pickup flags"),
                (None, None) => (BTreeSet::new(), "nothing (no inventory or flags)"),
            };
            if source_logged != Some(source) {
                log!("Class notes: detecting owned notes from {}", source);
                source_logged = Some(source);
            }
            crate::tracker::update(&|i| obtained(i, owned.as_ref(), flags.as_ref()));
            let shown: Vec<bool> = rows
                .iter()
                .map(|r| {
                    notes.contains(&r.note) && !r.items.iter().all(|i| obtained(i, owned.as_ref(), flags.as_ref()))
                })
                .collect();
            if last.as_ref().is_some_and(|(n, s)| *n == notes && *s == shown) {
                return;
            }
            for (r, &show) in rows.iter().zip(&shown) {
                let flag = if show { ALWAYS_ON_FLAG } else { ALWAYS_OFF_FLAG };
                unsafe { std::ptr::write_unaligned((r.row + EVENT_FLAG_OFFSET) as *mut u32, flag) };
            }
            let listed = rows.iter().filter(|r| notes.contains(&r.note)).count();
            let count = shown.iter().filter(|&&s| s).count();
            log!(
                "Class notes owned: {} | {} marker(s) shown, {} hidden as obtained",
                describe(&notes),
                count,
                listed - count
            );
            last = Some((notes, shown));
        },
        CSTaskGroupIndex::FrameBegin,
    );
    // The task runs for as long as its handle lives: keep it for the rest of the game.
    std::mem::forget(handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgraded_weapon_counts_as_affinity_and_plain() {
        // Heavy (affinity 1) +7 of weapon 2_000_000.
        assert_eq!(weapon_ids(2_000_107), [2_000_100, 2_000_000]);
        assert_eq!(weapon_ids(2_000_000), [2_000_000, 2_000_000]);
    }

    #[test]
    fn obtained_by_pickup_flag_or_ownership() {
        let owned = Owned { carried: HashSet::new(), items: [(3, 40_000)].into() };
        let armor = Item { category: 3, id: 40_000, flag: 0 };
        let other = Item { category: 3, id: 40_100, flag: 0 };
        assert!(obtained(&armor, Some(&owned), None));
        assert!(!obtained(&other, Some(&owned), None));
        assert!(!obtained(&armor, None, None));
    }

    #[test]
    fn only_carried_notes_count() {
        let note = NOTES[0].goods_id;
        let in_chest = Owned { carried: HashSet::new(), items: [(1, note)].into() };
        assert!(in_chest.notes().is_empty());
        let carried = Owned { carried: [(1, note)].into(), items: [(1, note)].into() };
        assert_eq!(carried.notes(), [note].into());
    }
}
