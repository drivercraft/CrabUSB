use alloc::string::String;

use crate::{
    err::TransferError,
    transfer::{Recipient, Request, RequestType},
};

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
    Other(#[from] anyhow::Error),
}

impl From<&str> for USBError {
    fn from(value: &str) -> Self {
        USBError::Other(anyhow::anyhow!("{value}"))
    }
}

impl From<String> for USBError {
    fn from(value: String) -> Self {
        USBError::Other(anyhow::anyhow!(value))
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
