//! Spin delay utilities for hardware initialization (CPU-speed dependent).

#[inline]
pub fn delay_us(us: u32) {
    for _ in 0..(us * 100) {
        core::hint::spin_loop();
    }
}

#[inline]
pub fn delay_ms(ms: u32) {
    for _ in 0..(ms as u64 * 100_000) {
        core::hint::spin_loop();
    }
}
