use alloc::{boxed::Box, string::String, vec::Vec};
use futures::future::LocalBoxFuture;

use crate::{
    descriptor::{ConfigurationDescriptor, DeviceDescriptor},
    err::TransferError,
    transfer::{Recipient, Request, RequestType, wait::Waiter},
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
    fn configuration_descriptor(
        &mut self,
        index: u8,
    ) -> LocalBoxFuture<'_, Result<ConfigurationDescriptor, USBError>>;
}

pub trait Device: Send + 'static {
    fn set_configuration(&mut self, configuration: u8) -> LocalBoxFuture<'_, Result<(), USBError>>;
    fn get_configuration(&mut self) -> LocalBoxFuture<'_, Result<u8, USBError>>;
    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> LocalBoxFuture<'_, Result<Box<dyn Interface>, USBError>>;

    fn string_descriptor(
        &mut self,
        index: u8,
        language_id: u16,
    ) -> LocalBoxFuture<'_, Result<String, USBError>>;
    fn control_in<'a>(&mut self, setup: ControlSetup, data: &'a mut [u8]) -> ResultTransfer<'a>;
    fn control_out<'a>(&mut self, setup: ControlSetup, data: &'a [u8]) -> ResultTransfer<'a>;
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
    fn endpoint_iso_in(&mut self, endpoint: u8) -> Result<Box<dyn EndpintIsoIn>, USBError>;
    fn endpoint_iso_out(&mut self, endpoint: u8) -> Result<Box<dyn EndpintIsoOut>, USBError>;
}

pub trait TEndpoint: Send + 'static {}

pub trait EndpointBulkIn: TEndpoint {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a>;
}
pub trait EndpointBulkOut: TEndpoint {
    fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a>;
}

pub trait EndpointInterruptIn: TEndpoint {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a>;
}

pub trait EndpointInterruptOut: TEndpoint {
    fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a>;
}

pub trait EndpintIsoIn: TEndpoint {
    fn submit<'a>(&mut self, data: &'a mut [u8], num_iso_packets: usize) -> ResultTransfer<'a>;
}

pub trait EndpintIsoOut: TEndpoint {
    fn submit<'a>(&mut self, data: &'a [u8], num_iso_packets: usize) -> ResultTransfer<'a>;
}

// pub type BoxTransfer<'a> = Pin<Box<dyn Transfer<'a> + Send>>;
pub type TransferFuture<'a> = Waiter<'a, Result<usize, TransferError>>;
pub type ResultTransfer<'a> = Result<TransferFuture<'a>, TransferError>;

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
    Other(String),
}

impl From<Box<dyn core::error::Error>> for USBError {
    fn from(err: Box<dyn core::error::Error>) -> Self {
        USBError::Other(alloc::format!("{err}"))
    }
}

#[derive(Debug, Clone)]
pub struct ControlSetup {
    pub request_type: RequestType,
    pub recipient: Recipient,
    pub request: Request,
    pub value: u16,
    pub index: u16,
}
