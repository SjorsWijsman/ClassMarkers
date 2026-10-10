//! Plain-text log next to the DLL (ClassMarkers.log), rewritten on every launch.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

static LOG: OnceLock<Mutex<File>> = OnceLock::new();

pub fn init(module: HMODULE) {
    let mut buf = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let mut path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
    path.set_extension("log");
    if let Ok(f) = OpenOptions::new().create(true).write(true).truncate(true).open(path) {
        let _ = LOG.set(Mutex::new(f));
    }
}

pub fn write(msg: &str) {
    if let Some(m) = LOG.get() {
        if let Ok(mut f) = m.lock() {
            let _ = writeln!(f, "{}", msg);
            let _ = f.flush();
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}
