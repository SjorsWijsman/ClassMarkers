//! Reading the game's event flags (the same lookup Map For Goblins uses).

use crate::scan;

/// `bool IsEventFlag(CSEventFlagMan*, u32* flag)`
const IS_EVENT_FLAG_AOB: &str = "48 83 EC 28 8B 12 85 D2";
/// `mov rdi, [rip+X]` loading the CSEventFlagMan singleton.
const EVENT_FLAG_MAN_AOB: &str = "48 8B 3D ?? ?? ?? ?? 48 85 FF ?? ?? 32 C0 E9";

pub struct Flags {
    is_event_flag: extern "C" fn(*const u8, *const u32) -> bool,
    man_slot: *const *const u8,
}

unsafe impl Send for Flags {}

impl Flags {
    pub fn resolve() -> Option<Flags> {
        let f = *scan::find_all(IS_EVENT_FLAG_AOB).first()?;
        let m = *scan::find_all(EVENT_FLAG_MAN_AOB).first()?;
        unsafe {
            Some(Flags {
                is_event_flag: std::mem::transmute(f),
                man_slot: scan::rip_relative(m, 3, 7) as *const *const u8,
            })
        }
    }

    /// None while the flag manager doesn't exist yet (no character loaded).
    pub fn get(&self, flag: u32) -> Option<bool> {
        let man = unsafe { *self.man_slot };
        if man.is_null() {
            return None;
        }
        Some((self.is_event_flag)(man, &flag))
    }
}
