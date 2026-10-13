#![allow(non_snake_case)]
//! Class Markers: adds map markers for the items listed in your Convergence class notes.
//!
//! Markers (generated from Convergence's files, see tools/generate.py) are added to
//! WorldMapPointParam at startup, labelled through the PlaceName text, and shown only while you own
//! the class note that lists them and haven't obtained the item yet. Nothing on disk is modified.
//! The approach for growing the marker table and text in memory follows Map For Goblins - DLL by
//! VirusAlex (MIT): https://github.com/VirusAlex/ERR-MapForGoblins-DLL

mod classes;
mod colors;
mod flags;
mod formats;
#[rustfmt::skip]
mod generated;
mod gfx;
mod iconart;
mod iconcache;
mod iconsource;
mod inject;
mod log;
mod markers;
mod mem;
mod msg;
mod noteicons;
mod notetext;
mod pixels;
mod scan;
mod tracker;

use std::sync::OnceLock;

/// Our own map icons, if they could be added (see gfx.rs).
static ICONS: OnceLock<gfx::MapIcons> = OnceLock::new();
/// Our class note icons for item descriptions (see noteicons.rs).
pub(crate) static NOTE_ICONS: OnceLock<noteicons::Available> = OnceLock::new();

use std::ffi::c_void;
use windows_sys::Win32::Foundation::{HMODULE, TRUE};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

#[no_mangle]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // Only the cheap bits here: DllMain runs under the loader lock. The file work happens in
        // modengine_ext_init, or on our thread if nothing calls that.
        log::init(module);
        std::thread::spawn(run);
    }
    TRUE
}

/// Mod Engine 2's init hook, which me3 calls right after loading us, on its loading thread and
/// outside the loader lock, before the game loads any menu. We use it to make our files in time;
/// no extension is handed back.
#[no_mangle]
pub extern "C" fn modengine_ext_init(_connector: *const c_void, _extension: *mut *mut c_void) -> bool {
    prepare_files("me3's init call");
    log!("Our files are ready (me3's init call waited for them)");
    true
}

/// Our icons have to be in place before the game loads its menus (file reads and writes; seconds
/// only when the note icons changed). Runs once: whichever of modengine_ext_init and our thread
/// comes first does it, and the other waits for it.
fn prepare_files(caller: &str) {
    static PREPARED: std::sync::Once = std::sync::Once::new();
    PREPARED.call_once(|| {
        log!("Preparing our files (from {})", caller);
        if let Some(d) = log::dll_dir() {
            iconcache::prepare(d); // our icons, made from Convergence's art (cached)
        }
        let icons = log::dll_dir().map(|d| gfx::prepare(d)).unwrap_or_default();
        let _ = ICONS.set(icons);
        let notes = log::dll_dir().map(|d| noteicons::prepare(d)).unwrap_or_default();
        let _ = NOTE_ICONS.set(notes);
    });
}

fn run() {
    log!("Class Markers {} loaded", env!("CARGO_PKG_VERSION"));
    prepare_files("our thread");
    let markers = generated::MARKERS;
    // Convergence's words (note entries, intros, item names), before we change any text.
    if let Err(e) = notetext::load() {
        log!("Couldn't read the class notes from the game's text ({}); labels will be short", e);
    }

    // Labels first: a marker must never point at text that doesn't exist.
    static ROWS: OnceLock<Vec<Option<usize>>> = OnceLock::new();
    // Text ids from FIRST_TEXT_ID on: each marker's label and hint lines in turn; `ids[i]` are
    // marker i's.
    let colours = log::dll_dir().map(|d| colors::class_colors(d)).cloned().unwrap_or_default();
    let mut labels = Vec::new();
    let mut ids: Vec<Vec<i32>> = Vec::new();
    for m in markers {
        let texts = m.texts(&colours);
        ids.push((0..texts.len()).map(|k| msg::FIRST_TEXT_ID + (labels.len() + k) as i32).collect());
        labels.extend(texts);
    }
    let have_labels = match msg::add_labels(labels, || {
        // Labels lost: blank every text line of ours so only the place-name lines remain.
        for &row in ROWS.get().into_iter().flatten().flatten() {
            unsafe { crate::markers::clear_texts_from(row, msg::FIRST_TEXT_ID) };
        }
    }) {
        Ok(()) => true,
        Err(e) => {
            log!("Item-name labels NOT added ({}); markers show place names", e);
            false
        }
    };

    let add: Vec<_> = markers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let texts = have_labels.then_some(ids[i].as_slice());
            (m.block, m.row(texts, ICONS.get().unwrap_or(&Default::default())), m.label.to_string())
        })
        .collect();
    match inject::inject(add) {
        Ok(rows) => {
            log!("Done: {} of {} marker(s) added to the world map", rows.iter().flatten().count(), markers.len());
            let watched = markers
                .iter()
                .zip(&rows)
                .filter_map(|(m, r)| r.map(|row| classes::Watched { note: m.note, items: m.items, row }))
                .collect();
            let _ = ROWS.set(rows);
            classes::watch(watched);
        }
        Err(e) => log!("Markers NOT added: {}", e),
    }

    match msg::add_class_lines(generated::CAPTIONS) {
        Ok(n) => log!("Added the class line to {} item descriptions", n),
        Err(e) => log!("Item descriptions NOT extended: {}", e),
    }
}
