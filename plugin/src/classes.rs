//! Show each marker only while you own the class note that lists it.
//!
//! Owning a note = it is in your inventory, read through the game's own data (GameDataMan) on
//! the game's thread, so dropping a note hides its markers again. If the inventory can't be
//! read, owning falls back to "any of the note's pickup flags is on" (see generated::NOTES).
//!
//! Our rows' eventFlagId is set to the always-on or always-off flag accordingly. The map reads it
//! when it is opened, so changes show the next time you open the map.

use crate::flags::Flags;
use crate::generated::NOTES;
use crate::log;
use crate::markers::{ALWAYS_OFF_FLAG, ALWAYS_ON_FLAG, EVENT_FLAG_OFFSET};
use eldenring::cs::{CSTaskGroupIndex, CSTaskImp, GameDataMan, ItemCategory};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::{FromStatic, SharedTaskImpExt};
use std::collections::BTreeSet;
use std::time::Duration;

/// Re-check every this many frames (about twice a second at 60 fps).
const CHECK_EVERY_FRAMES: u32 = 30;

/// The class notes in the main player's inventory, or None if it can't be read.
fn owned_from_inventory() -> Option<BTreeSet<u32>> {
    let game_data = unsafe { GameDataMan::instance() }.ok()?;
    let inventory = &game_data.main_player_game_data.equipment.equip_inventory_data.items_data;
    Some(
        inventory
            .items()
            .filter(|e| e.item_id.category() == ItemCategory::Goods)
            .map(|e| e.item_id.param_id())
            .filter(|id| NOTES.iter().any(|n| n.goods_id == *id))
            .collect(),
    )
}

/// The class notes whose pickup flags are on (fallback).
fn owned_from_flags(flags: &Flags) -> BTreeSet<u32> {
    NOTES
        .iter()
        .filter(|n| n.flags.iter().any(|&f| flags.get(f) == Some(true)))
        .map(|n| n.goods_id)
        .collect()
}

fn describe(owned: &BTreeSet<u32>) -> String {
    let names: Vec<String> = NOTES
        .iter()
        .filter(|n| owned.contains(&n.goods_id))
        .map(|n| format!("{} ({})", n.class, n.which))
        .collect();
    if names.is_empty() { "none".into() } else { names.join(", ") }
}

/// `rows`: (note goods id, address of the row's data in the live table) per marker.
pub fn watch(rows: Vec<(u32, usize)>) {
    let flags = Flags::resolve();
    let task = match CSTaskImp::wait_for_instance(Duration::from_secs(300)) {
        Ok(t) => t,
        Err(_) => {
            log!("Class notes: the game's task system was not found; markers stay hidden");
            return;
        }
    };
    let mut frame = 0u32;
    let mut last: Option<BTreeSet<u32>> = None;
    let mut source_logged = None;
    let handle = task.run_recurring(
        move |_: &FD4TaskData| {
            frame = frame.wrapping_add(1);
            if frame % CHECK_EVERY_FRAMES != 0 {
                return;
            }
            let (owned, source) = match owned_from_inventory() {
                Some(owned) => (owned, "inventory"),
                None => match &flags {
                    Some(f) => (owned_from_flags(f), "pickup flags"),
                    None => (BTreeSet::new(), "nothing (no inventory or flags)"),
                },
            };
            if source_logged != Some(source) {
                log!("Class notes: detecting owned notes from {}", source);
                source_logged = Some(source);
            }
            if last.as_ref() == Some(&owned) {
                return;
            }
            for &(note, row) in &rows {
                let flag = if owned.contains(&note) { ALWAYS_ON_FLAG } else { ALWAYS_OFF_FLAG };
                unsafe { std::ptr::write_unaligned((row + EVENT_FLAG_OFFSET) as *mut u32, flag) };
            }
            let shown = rows.iter().filter(|(n, _)| owned.contains(n)).count();
            log!("Class notes owned: {} | {} marker(s) shown", describe(&owned), shown);
            last = Some(owned);
        },
        CSTaskGroupIndex::FrameBegin,
    );
    // The task runs for as long as its handle lives: keep it for the rest of the game.
    std::mem::forget(handle);
}
