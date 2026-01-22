use core::any::Any;

use alloc::boxed::Box;
use alloc::vec::Vec;

use futures::future::{BoxFuture, LocalBoxFuture};
use usb_if::host::{USBError, hub::DeviceSpeed};

use crate::{
    backend::ty::{DeviceInfoOp, DeviceOp, EventHandlerOp, HubOp},
    hub::RouteString,
};

pub mod dwc;
#[cfg(libusb)]
pub mod libusb;
pub mod xhci;

pub(crate) mod ty;

define_int_type!(Dci, u8);
define_int_type!(PortId, usize);
define_int_type!(DeviceId, u32);

impl Dci {
    pub const CTRL: Self = Self(1);

    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

pub(crate) trait BackendOp: Send + Any + 'static {
    /// 初始化后端
    fn init<'a>(&'a mut self) -> BoxFuture<'a, Result<(), USBError>>;

    /// 探测已连接的设备
    fn device_list<'a>(&'a mut self)
    -> BoxFuture<'a, Result<Vec<Box<dyn DeviceInfoOp>>, USBError>>;

    fn open_device<'a>(
        &'a mut self,
        dev: &'a dyn DeviceInfoOp,
    ) -> LocalBoxFuture<'a, Result<Box<dyn DeviceOp>, USBError>>;

    fn create_event_handler(&mut self) -> Box<dyn EventHandlerOp>;
}

pub(crate) trait CoreOp: Send + 'static {
    /// 初始化后端
    fn init<'a>(&'a mut self) -> BoxFuture<'a, Result<(), USBError>>;

    fn root_hub(&mut self) -> Box<dyn HubOp>;

    fn new_addressed_device<'a>(
        &'a mut self,
        addr: DeviceAddressInfo,
    ) -> BoxFuture<'a, Result<Box<dyn DeviceOp>, USBError>>;

    fn create_event_handler(&mut self) -> Box<dyn EventHandlerOp>;

    fn kernel(&self) -> &'static dyn crate::osal::KernelOp;
}

pub struct DeviceAddressInfo {
    pub route_string: RouteString,
    pub root_port_id: u8,
    pub parent_hub_slot_id: u8,
    pub port_speed: DeviceSpeed,
    /// TT 信息：设备在 Hub 上的端口号（LS/FS 设备需要）
    pub tt_port_on_hub: Option<u8>,
}
