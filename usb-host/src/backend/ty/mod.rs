use core::{fmt::Debug, future::Future};

use usb_if::{
    descriptor::{ConfigurationDescriptor, DeviceDescriptor},
    err::TransferError,
    host::ControlSetup,
};

use crate::{backend::ty::ep::EndpointControl, err::USBError};

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
    fn descriptor(&self) -> &DeviceDescriptor;

    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> impl Future<Output = Result<(), USBError>> + Send;

    fn ep_ctrl(&mut self) -> &mut EndpointControl;

    fn set_configuration(
        &mut self,
        configuration_value: u8,
    ) -> impl Future<Output = Result<(), USBError>> + Send;

    async fn control_in(
        &mut self,
        param: ControlSetup,
        buff: &mut [u8],
    ) -> core::result::Result<usize, TransferError> {
        self.ep_ctrl().control_in(param, buff).await
    }

    async fn control_out(
        &mut self,
        param: ControlSetup,
        buff: &[u8],
    ) -> core::result::Result<usize, TransferError> {
        self.ep_ctrl().control_out(param, buff).await
    }

    // async fn new_endpoint(&mut self, dci: Dci) -> Result<Self::Ep, USBError>;
}
