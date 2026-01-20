use alloc::vec::Vec;
use futures::future::BoxFuture;

use usb_if::host::USBError;

use crate::hub::DeviceAddressInfo;

pub trait HubOp: Send + 'static {
    fn reset(&mut self) -> Result<(), USBError>;
    fn changed_ports<'a>(&'a mut self) -> BoxFuture<'a, Result<Vec<DeviceAddressInfo>, USBError>>;
}
