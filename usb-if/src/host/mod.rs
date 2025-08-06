use core::pin::Pin;

use alloc::{boxed::Box, vec::Vec};
use futures::{FutureExt, future::LocalBoxFuture};

use crate::{
    descriptor::{
        ConfigurationDescriptor, DeviceDescriptor, EndpointDescriptor, InterfaceDescriptor,
    },
    err::TransferError,
    transfer::{Recipient, Request, RequestType},
};

pub trait Controller: Send + 'static {
    fn init(&mut self) -> LocalBoxFuture<'_, Result<(), USBError>>;
    fn device_list(&self) -> LocalBoxFuture<'_, Result<Vec<Box<dyn DeviceInfo>>, USBError>>;

    /// Used in interrupt context.
    fn handle_event(&mut self);
}

pub trait DeviceInfo: Send + 'static {
    fn open(&mut self) -> LocalBoxFuture<'_, Result<Box<dyn Device>, USBError>>;
    fn descriptor(&self) -> LocalBoxFuture<'_, Result<DeviceDescriptor, USBError>>;
    fn configuration_descriptors(
        &self,
    ) -> LocalBoxFuture<'_, Result<Vec<ConfigurationDescriptor>, USBError>>;
}

pub trait Device: Send + 'static {
    fn set_configuration(&mut self, configuration: u8) -> LocalBoxFuture<'_, Result<(), USBError>>;
    fn get_configuration(&mut self) -> LocalBoxFuture<'_, Result<u8, USBError>>;
    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> LocalBoxFuture<'_, Result<Box<dyn Interface>, USBError>>;

    fn configuration_descriptors(
        &mut self,
    ) -> LocalBoxFuture<'_, Result<Vec<ConfigurationDescriptor>, USBError>>;

    fn current_configuration_descriptor(
        &mut self,
    ) -> LocalBoxFuture<'_, Result<ConfigurationDescriptor, USBError>> {
        async move {
            let value = self.get_configuration().await?;
            if value == 0 {
                Err(USBError::ConfigurationNotSet)
            } else {
                let descs = self.configuration_descriptors().await?;
                for desc in descs {
                    if desc.configuration_value == value {
                        return Ok(desc);
                    }
                }
                Err(USBError::NotFound)
            }
        }
        .boxed_local()
    }
}

pub trait Interface: Send + 'static {
    fn set_alt_setting(&mut self, alt_setting: u8) -> Result<(), USBError>;
    fn get_alt_setting(&self) -> Result<u8, USBError>;
    fn control_in<'a>(&mut self, setup: ControlSetup, data: &'a mut [u8]) -> ResultTransfer<'a>;
    fn control_out<'a>(&mut self, setup: ControlSetup, data: &'a [u8]) -> ResultTransfer<'a>;
    fn endpoint_bulk_in(&mut self, endpoint: u8) -> Result<Box<dyn EndpointBulkIn>, USBError>;
    fn endpoint_bulk_out(&mut self, endpoint: u8) -> Result<Box<dyn EndpointBulkOut>, USBError>;
    fn endpoint_interrupt_in(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn EndpointInterruptIn>, USBError>;
    fn endpoint_interrupt_out(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn EndpointInterruptOut>, USBError>;
    fn descriptor(&self) -> &InterfaceDescriptor;
}

pub trait TEndpint: Send + 'static {
    fn descriptor(&self) -> &EndpointDescriptor;
}

pub trait EndpointBulkIn: TEndpint {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a>;
}
pub trait EndpointBulkOut: TEndpint {
    fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a>;
}

pub trait EndpointInterruptIn: TEndpint {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a>;
}

pub trait EndpointInterruptOut: TEndpint {
    fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a>;
}

pub type BoxTransfer<'a> = Pin<Box<dyn Transfer<'a> + Send>>;
pub type ResultTransfer<'a> = Result<BoxTransfer<'a>, TransferError>;

pub trait Transfer<'a>: Future<Output = Result<usize, TransferError>> + Send + 'a {}

#[derive(thiserror::Error, Debug)]
pub enum USBError {
    #[error("Timeout")]
    Timeout,
    #[error("No memory available")]
    NoMemory,
    #[error("Transfer error: {0}")]
    TransferError(#[from] TransferError),
    #[error("Not initialized")]
    NotInitialized,
    #[error("Not found")]
    NotFound,
    #[error("Slot limit reached")]
    SlotLimitReached,
    #[error("Configuration not set")]
    ConfigurationNotSet,
    #[error("Other error: {0}")]
    Other(#[from] Box<dyn core::error::Error>),
}

#[derive(Debug, Clone)]
pub struct ControlSetup {
    pub request_type: RequestType,
    pub recipient: Recipient,
    pub request: Request,
    pub value: u16,
    pub index: u16,
}
