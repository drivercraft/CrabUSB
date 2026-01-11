//! DWC3 (DesignWare USB3 Controller) detection for RK3588.

use core::ptr::NonNull;

const DWC3_REG_OFFSET: usize = 0xC100;
const DWC3_GSNPSID_MASK: u32 = 0xFFFF0000;
const DWC3_GSNPSID_CORE_3: u32 = 0x55330000;
const DWC3_GSNPSID_CORE_31: u32 = 0x33310000;

/// # Safety
/// The caller must ensure the XHCI base address is valid and properly mapped.
pub unsafe fn is_dwc3_xhci(xhci_base: NonNull<u8>) -> bool {
    unsafe {
        let gsnpsid_addr = xhci_base.as_ptr().add(DWC3_REG_OFFSET + 0x20) as *const u32;
        let id = gsnpsid_addr.read_volatile();
        let masked = id & DWC3_GSNPSID_MASK;
        masked == DWC3_GSNPSID_CORE_3 || masked == DWC3_GSNPSID_CORE_31
    }
}
