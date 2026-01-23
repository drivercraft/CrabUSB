use core::any::Any;
use core::fmt::Debug;

use futures::future::BoxFuture;
use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor};
use usb_if::host::hub::Speed;

use crate::{backend::ty::ep::EndpointControl, err::USBError};

// pub mod hub;
pub mod ep;
mod hub;
pub mod transfer;

pub use hub::*;

#[derive(Debug, Clone)]
pub enum Event {
    Nothing,
    PortChange { port: u8 },
    Stopped,
}

pub(crate) trait EventHandlerOp: Send + Any + Sync + 'static {
    fn handle_event(&self) -> Event;
}

pub(crate) trait DeviceInfoOp: Send + Sync + Any + Debug + 'static {
    fn id(&self) -> usize;
    fn backend_name(&self) -> &str;
    fn descriptor(&self) -> &DeviceDescriptor;
    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor];
}

/// USB 设备特征（高层抽象）
pub(crate) trait DeviceOp: Send + Any + 'static {
    fn id(&self) -> usize;
    fn backend_name(&self) -> &str;
    fn descriptor(&self) -> &DeviceDescriptor;
    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor];

    fn claim_interface<'a>(
        &'a mut self,
        interface: u8,
        alternate: u8,
    ) -> BoxFuture<'a, Result<(), USBError>>;

    fn ep_ctrl(&mut self) -> &mut EndpointControl;

    fn set_configuration<'a>(
        &'a mut self,
        configuration_value: u8,
    ) -> BoxFuture<'a, Result<(), USBError>>;

    fn get_endpoint(
        &mut self,
        desc: &usb_if::descriptor::EndpointDescriptor,
    ) -> Result<ep::EndpointBase, USBError>;

    fn update_hub(&mut self, params: HubParams) -> BoxFuture<'_, Result<(), USBError>>;

    // async fn new_endpoint(&mut self, dci: Dci) -> Result<Self::Ep, USBError>;
}

#[derive(Debug, Clone)]
pub struct HubParams {
    /// Hub 端口数量
    pub num_ports: u8,

    /// 是否为 Multi-TT Hub
    pub multi_tt: bool,

    /// TT 思考时间（单位：纳秒）
    /// 8 FS bit times = 666ns
    pub tt_think_time_ns: u16,

    /// 父 Hub Slot ID（0 表示 Root Hub）
    pub parent_hub_slot_id: u8,

    /// Root Hub 端口号
    pub root_hub_port_number: u8,

    /// 设备速度
    pub port_speed: Speed,
}
