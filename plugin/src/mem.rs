//! Reading and writing values at raw addresses in the game's memory (no alignment needed).

pub unsafe fn rd<T: Copy>(p: *const u8) -> T {
    std::ptr::read_unaligned(p as *const T)
}

pub unsafe fn wr<T>(p: *mut u8, v: T) {
    std::ptr::write_unaligned(p as *mut T, v)
}
