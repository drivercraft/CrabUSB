use alloc::vec::Vec;
use core::fmt::Debug;
use core::future::Future;

use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor};

use crate::err::USBError;

// pub mod hub;

pub trait HostOp {
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

#[derive(Debug, Clone)]
pub enum Event {
    Nothing,
    PortChange { port: u8 },
}

pub trait EventHandlerOp: Send + Sync + 'static {
    fn handle_event(&self) -> Event;
}

pub trait DeviceInfoOp: Send + Debug + 'static {
    type Device: DeviceOp;
    fn descriptor(&self) -> &DeviceDescriptor;
    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor];
}

/// USB 设备特征（高层抽象）
pub trait DeviceOp: Send + 'static {
    type Req: TransferReq;
    type Res: TransferRes;
    // type Ep: Endpint<Req = Self::Req, Res = Self::Res>;

    fn descriptor(&self) -> &DeviceDescriptor;

    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> impl Future<Output = Result<(), USBError>> + Send;

    fn set_configuration(
        &mut self,
        configuration_value: u8,
    ) -> impl Future<Output = Result<(), USBError>> + Send;

    // async fn new_endpoint(&mut self, dci: Dci) -> Result<Self::Ep, USBError>;
}

pub trait TransferReq {}
pub trait TransferRes {}

// pub trait Endpint: Send + 'static {
//     type Req: TransferReq;
//     type Res: TransferRes;

//     /// 提交传输
//     fn submit(&mut self, req: Self::Req) -> Result<Self::Res, USBError>;
// }
