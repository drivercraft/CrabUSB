use usb_if::descriptor::DeviceDescriptor;

use crate::backend::{
    ty::{DeviceInfoOp, DeviceOp},
    xhci::SlotId,
};

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    slot_id: SlotId,
    desc: DeviceDescriptor,
}

impl DeviceInfo {
    pub fn new(slot_id: SlotId, desc: DeviceDescriptor) -> Self {
        Self { slot_id, desc }
    }

    pub fn slot_id(&self) -> SlotId {
        self.slot_id
    }
}

impl DeviceInfoOp for DeviceInfo {
    type Device = Device;

    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
}

pub struct Device {}

impl DeviceOp for Device {
    type Req = super::TransferRequest;

    type Res = super::TransferResult;

    // type Ep;

    async fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> Result<(), usb_if::host::USBError> {
        todo!()
    }

    // async fn new_endpoint(&mut self, dci: Dci) -> Result<Self::Ep, usb_if::host::USBError> {
    //     todo!()
    // }
}
