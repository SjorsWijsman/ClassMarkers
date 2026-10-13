//! Reading the game's event flags (the same lookup Map For Goblins uses).

use crate::scan;

/// `bool IsEventFlag(CSEventFlagMan*, u32* flag)`
const IS_EVENT_FLAG_AOB: &str = "48 83 EC 28 8B 12 85 D2";
/// `mov rdi, [rip+X]` loading the CSEventFlagMan singleton.
const EVENT_FLAG_MAN_AOB: &str = "48 8B 3D ?? ?? ?? ?? 48 85 FF ?? ?? 32 C0 E9";

type IsEventFlag = extern "C" fn(*const u8, *const u32) -> bool;

pub struct Flags {
    is_event_flag: IsEventFlag,
    man_slot: *const *const u8,
}

unsafe impl Send for Flags {}

impl Flags {
    pub fn resolve() -> Result<Flags, String> {
        let f = scan::find_unique("event flag lookup", IS_EVENT_FLAG_AOB)?;
        let man_slot = scan::find_global("event flag manager", EVENT_FLAG_MAN_AOB, 3, 7)? as *const *const u8;
        let is_event_flag = unsafe { std::mem::transmute::<*const u8, IsEventFlag>(f) };
        Ok(Flags { is_event_flag, man_slot })
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
