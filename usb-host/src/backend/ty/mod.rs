use core::fmt::Debug;
use core::future::Future;
use core::pin::Pin;

use usb_if::{
    descriptor::{ConfigurationDescriptor, DeviceDescriptor},
    err::TransferError,
};

use crate::{
    backend::ty::{
        ep::EndpointOp,
        transfer::{Transfer, TransferKind},
    },
    err::USBError,
};

// pub mod hub;
pub mod ep;
pub mod transfer;

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
    type Ep: EndpointOp;

    fn descriptor(&self) -> &DeviceDescriptor;

    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> impl Future<Output = Result<(), USBError>> + Send;

    fn ep_ctrl(&mut self) -> &mut Self::Ep;

    fn set_configuration(
        &mut self,
        configuration_value: u8,
    ) -> impl Future<Output = Result<(), USBError>> + Send;

    // async fn new_endpoint(&mut self, dci: Dci) -> Result<Self::Ep, USBError>;
}
