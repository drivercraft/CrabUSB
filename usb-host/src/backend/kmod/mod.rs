use crate::{Mmio, USBHost};

mod dwc;
mod hub;
mod kcore;
pub mod osal;
mod transfer;
mod xhci;

use crate::err::*;

use alloc::boxed::Box;

use dwc::Dwc;
use hub::RouteString;
use kcore::*;
use usb_if::Speed;
use xhci::Xhci;

pub use dwc::{
    CruOp, DwcNewParams, DwcParams, UdphyParam, Usb2PhyParam, UsbPhyInterfaceMode,
    usb2phy::Usb2PhyPortId,
};
pub use osal::*;

impl USBHost {
    pub fn new_xhci(mmio: Mmio, kernel: &'static dyn KernelOp) -> Result<USBHost> {
        Ok(USBHost::new(Xhci::new(mmio, kernel)?))
    }

    pub fn new_dwc(params: DwcNewParams<'_, impl CruOp>) -> Result<USBHost> {
        Ok(USBHost::new(Dwc::new(params)?))
    }

    pub(crate) fn new(backend: impl CoreOp) -> Self {
        let b = Core::new(backend);
        Self {
            backend: Box::new(b),
        }
    }
}

pub struct DeviceAddressInfo {
    pub route_string: RouteString,
    pub root_port_id: u8,
    pub parent_hub_slot_id: u8,
    pub port_speed: Speed,
    /// TT 信息：设备在 Hub 上的端口号（LS/FS 设备需要）
    pub tt_port_on_hub: Option<u8>,
}
