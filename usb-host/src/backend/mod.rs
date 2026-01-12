use alloc::{boxed::Box, vec::Vec};

use usb_if::host::USBError;

use crate::{
    Dwc, Xhci,
    backend::ty::{DeviceInfoOp, DeviceOp, EventHandlerOp},
};

// #[cfg(feature = "libusb")]
// pub mod libusb;
pub mod dwc;
pub mod xhci;

pub(crate) mod ty;

define_int_type!(Dci, u8);
define_int_type!(PortId, usize);

impl Dci {
    pub const CTRL: Self = Self(1);

    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

pub trait BackendOp {
    type DeviceInfo: DeviceInfoOp;
    type EventHandler: EventHandlerOp;

    /// 初始化后端
    fn init(&mut self) -> impl Future<Output = Result<(), USBError>> + Send;

    /// 探测已连接的设备
    fn probe_devices(
        &mut self,
    ) -> impl Future<Output = Result<Vec<Self::DeviceInfo>, USBError>> + Send;

    fn open_device(
        &mut self,
        dev: &Self::DeviceInfo,
    ) -> impl Future<Output = Result<<Self::DeviceInfo as DeviceInfoOp>::Device, USBError>> + Send;

    fn create_event_handler(&mut self) -> Self::EventHandler;
}
