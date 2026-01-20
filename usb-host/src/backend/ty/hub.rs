use alloc::vec::Vec;
use futures::future::LocalBoxFuture;

use usb_if::host::USBError;

use crate::hub::DeviceAddressInfo;

pub trait HubOp: Send + 'static {
    fn reset(&mut self) -> Result<(), USBError>;
    fn changed_ports(&mut self) -> LocalBoxFuture<'_, Result<Vec<DeviceAddressInfo>, USBError>>;
}
