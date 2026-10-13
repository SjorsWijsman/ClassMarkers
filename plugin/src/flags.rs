//! Reading the game's event flags through its CSEventFlagMan (fromsoftware-rs).

use eldenring::cs::CSEventFlagMan;
use fromsoftware_shared::FromStatic;

/// The game's event flags as they are now. Use on the game's thread only (classes.rs).
pub struct Flags(&'static CSEventFlagMan);

impl Flags {
    /// None while the flag manager doesn't exist (no character loaded).
    pub fn current() -> Option<Flags> {
        unsafe { CSEventFlagMan::instance() }.ok().map(Flags)
    }

    pub fn get(&self, flag: u32) -> bool {
        self.0.virtual_memory_flag.get_flag(flag)
    }
}
