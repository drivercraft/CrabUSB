use crate::backend::ty::DeviceOp;

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
