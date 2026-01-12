use alloc::vec::Vec;
use core::fmt::Debug;
use core::future::Future;

use usb_if::{
    descriptor::{ConfigurationDescriptor, DeviceDescriptor},
    err::TransferError,
    host::ControlSetup,
    transfer::Direction,
};

use crate::err::USBError;

// pub mod hub;
pub mod ep;



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
    type Ep: EndpintOp;

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

pub trait EndpintOp: Send + 'static {
    type Transfer: TransferOp;
}

pub trait TransferOp: Send + 'static {
    fn data_ptr(&self) -> usize;
    fn data_len(&self) -> usize;
}
