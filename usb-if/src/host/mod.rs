use core::pin::Pin;

use alloc::{boxed::Box, string::String, vec::Vec};
use futures::future::LocalBoxFuture;

use crate::{
    descriptor::DeviceDescriptor,
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
}

pub trait Device: Send + 'static {
    fn set_configuration(&mut self, configuration: u8) -> LocalBoxFuture<'_, Result<(), USBError>>;
    fn get_configuration(&self) -> LocalBoxFuture<'_, Result<u8, USBError>>;
    fn claim_interface(
        &mut self,
        interface: u8,
    ) -> LocalBoxFuture<'_, Result<Box<dyn Interface>, USBError>>;
}

pub trait Interface: Send + 'static {
    fn set_alt_setting(&mut self, alt_setting: u8) -> Result<(), USBError>;
    fn get_alt_setting(&self) -> Result<u8, USBError>;
    fn control_in(&mut self, setup: ControlSetup, data: &'_ mut [u8]) -> ResultTransfer<'_>;
    fn control_out(&mut self, setup: ControlSetup, data: &'_ [u8]) -> ResultTransfer<'_>;
    fn endpoint_bulk_in(&mut self, endpoint: u8) -> Result<Box<dyn EndpointBulkIn>, USBError>;
}

pub trait EndpointBulkIn: Send + 'static {
    fn submit(&mut self, data: &'_ mut [u8]) -> ResultTransfer<'_>;
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
