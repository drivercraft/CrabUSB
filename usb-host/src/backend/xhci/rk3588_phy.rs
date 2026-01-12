//! RK3588 GPIO VBUS control for GL3523 hub workaround.

use core::ptr::write_volatile;

use super::delay::delay_ms;

const GPIO3_BASE: usize = 0xFEC40000;
const GPIO_SWPORT_DR_L: usize = 0x0000;
const GPIO_SWPORT_DDR_L: usize = 0x0008;
const GPIO3_B7_BIT: u32 = 1 << 15;
const WRITE_MASK_BIT15: u32 = 1 << 31;

pub const VBUS_OFF_MS: u32 = 1000;
pub const VBUS_ON_WAIT_MS: u32 = 500;

const RK3588_USB3_PORT1_BASE: usize = 0xFC400000;

pub fn is_rk3588_usb3_port1(xhci_base: usize) -> bool {
    xhci_base == RK3588_USB3_PORT1_BASE
}

/// # Safety
/// Direct hardware register access. Caller must ensure this is RK3588 hardware.
pub unsafe fn toggle_vbus_port1(off_ms: u32, on_wait_ms: u32) {
    unsafe {
        let ddr_ptr = (GPIO3_BASE + GPIO_SWPORT_DDR_L) as *mut u32;
        write_volatile(ddr_ptr, WRITE_MASK_BIT15 | GPIO3_B7_BIT);

        let dr_ptr = (GPIO3_BASE + GPIO_SWPORT_DR_L) as *mut u32;
        write_volatile(dr_ptr, WRITE_MASK_BIT15);
    }
    delay_ms(off_ms);

    unsafe {
        let dr_ptr = (GPIO3_BASE + GPIO_SWPORT_DR_L) as *mut u32;
        write_volatile(dr_ptr, WRITE_MASK_BIT15 | GPIO3_B7_BIT);
    }
    delay_ms(on_wait_ms);
}
