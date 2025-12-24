use alloc::vec::Vec;

use super::Device;
use crate::{Mmio, backend::ty::HostOp};

pub struct Xhci {}

impl Xhci {
    pub fn new(mmio: Mmio, dma_mask: usize) -> Self {
        Xhci {}
    }
}

impl HostOp for Xhci {
    type Device = Device;

    async fn initialize(&mut self) -> Result<(), usb_if::host::USBError> {
        todo!()
    }

    async fn device_list(
        &self,
    ) -> Result<Vec<usb_if::descriptor::DeviceDescriptor>, usb_if::host::USBError> {
        todo!()
    }

    async fn open_device(
        &mut self,
        desc: &usb_if::descriptor::DeviceDescriptor,
    ) -> Result<Self::Device, usb_if::host::USBError> {
        todo!()
    }

    fn poll_events(&mut self) {
        todo!()
    }
}
