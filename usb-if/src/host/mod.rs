use core::pin::Pin;

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::{
    descriptor::DeviceDescriptor,
    transfer::{Recipient, Request, RequestType, TransferError},
};

pub trait Controller: Send + 'static {
    fn init(&mut self) -> Result<(), UsbError>;
    fn device_list(&self) -> Result<Vec<Box<dyn DeviceInfo>>, UsbError>;
}

pub trait DeviceInfo: Send + 'static {
    fn open(&mut self) -> Result<Box<dyn Device>, UsbError>;
    fn descriptor(&self) -> Result<DeviceDescriptor, UsbError>;
}

pub trait Device: Send + 'static {
    fn set_configuration(&mut self, configuration: u8) -> Result<(), UsbError>;
    fn get_configuration(&self) -> Result<u8, UsbError>;
    fn claim_interface(&mut self, interface: u8) -> Result<Box<dyn Interface>, UsbError>;
}

pub trait Interface: Send + 'static {
    fn set_alt_setting(&mut self, alt_setting: u8) -> Result<(), UsbError>;
    fn get_alt_setting(&self) -> Result<u8, UsbError>;
    fn control_in<'a>(&mut self, setup: ControlSetup, data: &'a mut [u8]) -> ResultTransfer<'a>;
    fn control_out<'a>(&mut self, setup: ControlSetup, data: &'a [u8]) -> ResultTransfer<'a>;
    fn endpoint_bulk_in(&mut self, endpoint: u8) -> Result<Box<dyn EndpointBulkIn>, UsbError>;
}

pub trait EndpointBulkIn: Send + 'static {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a>;
}

pub type BoxTransfer<'a> = Pin<Box<dyn Transfer<'a> + Send>>;
pub type ResultTransfer<'a> = Result<BoxTransfer<'a>, TransferError>;

pub trait Transfer<'a>: Future<Output = Result<usize, TransferError>> + Send + 'a {}

#[derive(thiserror::Error, Debug)]
pub enum UsbError {
    #[error("Timeout")]
    Timeout,
    #[error("No memory available")]
    NoMemory,
    #[error("Other error: {0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct ControlSetup {
    pub request_type: RequestType,
    pub recipient: Recipient,
    pub request: Request,
    pub value: u16,
    pub index: u16,
}
