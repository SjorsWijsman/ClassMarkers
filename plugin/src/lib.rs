#![allow(non_snake_case)]
//! Class Markers: adds map markers for the items listed in your Convergence class notes.
//!
//! Proof of concept: injects a fixed set of test markers into WorldMapPointParam at runtime.
//! Nothing on disk is modified. The approach for growing the marker table in memory follows
//! Map For Goblins - DLL by VirusAlex (MIT): https://github.com/VirusAlex/ERR-MapForGoblins-DLL

#[allow(dead_code)] // for class detection, next step
mod flags;
#[allow(dead_code)] // CLASSES is for class detection, next step
mod generated;
mod inject;
mod log;
mod markers;
mod msg;
mod scan;

use std::sync::OnceLock;

use std::ffi::c_void;
use windows_sys::Win32::Foundation::{HMODULE, TRUE};
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        log::init(module);
        // Never do real work under the loader lock: hand off to our own thread.
        std::thread::spawn(run);
    }
    TRUE
}

fn run() {
    log!("Class Markers {} loaded", env!("CARGO_PKG_VERSION"));
    // Until class detection is in: show the Nox Oracle's markers only.
    const TEST_CLASS_NOTE: u32 = 8812;
    let chosen: Vec<&markers::GenMarker> =
        generated::MARKERS.iter().filter(|m| m.note == TEST_CLASS_NOTE).collect();

    // Labels first: a marker must never point at text that doesn't exist.
    static ROWS: OnceLock<Vec<usize>> = OnceLock::new();
    let labels = chosen.iter().map(|m| m.name.to_string()).collect();
    let have_labels = match msg::add_labels(labels, || {
        // Labels lost: blank line 1 of our rows so their place-name line shows instead.
        for &row in ROWS.get().into_iter().flatten() {
            unsafe { std::ptr::write_unaligned((row + 0x30) as *mut i32, -1) };
        }
    }) {
        Ok(()) => true,
        Err(e) => {
            log!("Item-name labels NOT added ({}); markers show place names", e);
            false
        }
    };

    let add: Vec<_> = chosen
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let label = have_labels.then_some(msg::FIRST_TEXT_ID + i as i32);
            (m.block, m.row(label), m.label.to_string())
        })
        .collect();
    match inject::inject(add) {
        Ok(rows) => {
            log!("Done: {} marker(s) added to the world map", rows.len());
            let _ = ROWS.set(rows);
        }
        Err(e) => log!("Markers NOT added: {}", e),
    }
}
