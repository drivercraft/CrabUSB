use alloc::vec::Vec;
use core::any::Any;
use futures::future::BoxFuture;

use usb_if::host::USBError;

use crate::hub::DeviceAddressInfo;

pub trait HubOp: Send + 'static + Any {
    fn reset(&mut self) -> Result<(), USBError>;
    fn changed_ports<'a>(&'a mut self) -> BoxFuture<'a, Result<Vec<DeviceAddressInfo>, USBError>>;

    /// 支持 downcast（用于访问具体类型的方法）
    fn as_any(&self) -> &dyn Any;
}
