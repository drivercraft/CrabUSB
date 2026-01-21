use alloc::vec::Vec;
use core::any::Any;
use futures::future::BoxFuture;

use usb_if::host::USBError;

use crate::hub::PortChangeInfo;

pub trait HubOp: Send + 'static + Any {
    fn init(&mut self) -> Result<(), USBError>;
    fn setup<'a>(&'a mut self) -> BoxFuture<'a, Result<(), USBError>>;
    fn changed_ports<'a>(&'a mut self) -> BoxFuture<'a, Result<Vec<PortChangeInfo>, USBError>>;
}
