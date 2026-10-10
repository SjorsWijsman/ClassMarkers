#![allow(non_snake_case)]
//! Class Markers: adds map markers for the items listed in your Convergence class notes.
//!
//! Markers (generated from Convergence's files, see tools/generate.py) are added to
//! WorldMapPointParam at startup, labelled through the PlaceName text, and shown only while you
//! own the class note that lists them and haven't obtained the item yet. Nothing on disk is modified. The approach for growing the
//! marker table and text in memory follows Map For Goblins - DLL by VirusAlex (MIT):
//! https://github.com/VirusAlex/ERR-MapForGoblins-DLL

mod classes;
mod flags;
mod generated;
mod gfx;
mod inject;
mod log;
mod markers;
mod msg;
mod scan;

use std::sync::OnceLock;

/// Our own map icons, if they could be added (see gfx.rs).
static ICONS: OnceLock<Option<gfx::IconIds>> = OnceLock::new();

use std::ffi::c_void;
use windows_sys::Win32::Foundation::{HMODULE, TRUE};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        log::init(module);
        // Our icons have to be in place before the game loads its menus, so this one small job
        // (a file read and write) runs right here; everything else goes to our own thread.
        let icons = log::dll_dir().and_then(|d| gfx::prepare(d));
        let _ = ICONS.set(icons);
        std::thread::spawn(run);
    }
    TRUE
}

fn run() {
    log!("Class Markers {} loaded", env!("CARGO_PKG_VERSION"));
    let markers = generated::MARKERS;

    // Labels first: a marker must never point at text that doesn't exist.
    static ROWS: OnceLock<Vec<Option<usize>>> = OnceLock::new();
    let labels = markers.iter().map(|m| m.label_text()).collect();
    let have_labels = match msg::add_labels(labels, || {
        // Labels lost: blank every text line of ours so only the place-name lines remain.
        for &row in ROWS.get().into_iter().flatten().flatten() {
            for k in 0..8 {
                let at = (row + 0x30 + 12 * k) as *mut i32;
                if unsafe { std::ptr::read_unaligned(at) } >= msg::FIRST_TEXT_ID {
                    unsafe { std::ptr::write_unaligned(at, -1) };
                }
            }
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
            let label = have_labels.then_some(msg::FIRST_TEXT_ID + i as i32);
            (m.block, m.row(label, ICONS.get().copied().flatten()), m.label.to_string())
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
