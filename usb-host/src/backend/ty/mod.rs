use core::any::Any;
use core::fmt::Debug;

use futures::future::BoxFuture;
use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor};

use crate::{backend::ty::ep::EndpointControl, err::USBError};

// pub mod hub;
pub mod ep;
pub mod transfer;
mod hub;

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
    fn backend_name(&self) -> &str;
    fn descriptor(&self) -> &DeviceDescriptor;
    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor];
}

/// USB 设备特征（高层抽象）
pub(crate) trait DeviceOp: Send + Any + 'static {
    fn backend_name(&self) -> &str;
    fn parent_port_id(&self) -> Option<u8>;
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

    // async fn new_endpoint(&mut self, dci: Dci) -> Result<Self::Ep, USBError>;
}
