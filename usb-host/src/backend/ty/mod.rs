use alloc::vec::Vec;
use core::future::Future;

use usb_if::descriptor::DeviceDescriptor;

use crate::err::USBError;

// pub mod hub;

// ============================================================================
// 高层后端抽象 (Backend Trait)
// ============================================================================

/// 后端控制器特征
///
/// 这是高层抽象，用于连接 HCD 层和 usb-if 接口层
pub trait HostOp {
    type Device: DeviceOp;
    type EventHandler: EventHandlerOp;

    /// 初始化后端
    fn init(&mut self) -> impl Future<Output = Result<(), USBError>> + Send;

    /// 获取设备列表
    fn device_list(&self) -> impl Future<Output = Result<Vec<DeviceDescriptor>, USBError>> + Send;

    /// 打开指定设备
    fn open_device(
        &mut self,
        desc: &DeviceDescriptor,
    ) -> impl Future<Output = Result<Self::Device, USBError>> + Send;

    fn create_event_handler(&mut self) -> Self::EventHandler;
}

pub trait EventHandlerOp: Send + Sync + 'static {
    fn handle_event(&self);
}

/// USB 设备特征（高层抽象）
pub trait DeviceOp: Send + 'static {
    type Req: TransferReq;
    type Res: TransferRes;
    // type Ep: Endpint<Req = Self::Req, Res = Self::Res>;

    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> impl Future<Output = Result<(), USBError>> + Send;

    // async fn new_endpoint(&mut self, dci: Dci) -> Result<Self::Ep, USBError>;
}

pub trait TransferReq {}
pub trait TransferRes {}

pub trait Endpint: Send + 'static {
    type Req: TransferReq;
    type Res: TransferRes;

    /// 提交传输
    fn submit(&mut self, req: Self::Req) -> Result<Self::Res, USBError>;
}
