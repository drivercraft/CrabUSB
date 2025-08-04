use thiserror::Error;

#[derive(Error, Debug)]
pub enum USBError {
    #[error("unknown usb error")]
    Unknown,
    #[error("not initialized")]
    NotInitialized,
    #[error("no memory")]
    NoMemory,
    #[error("slot limit reached")]
    SlotLimitReached,
    #[error("transfer event error: {0:?}")]
    TransferEventError(xhci::ring::trb::event::CompletionCode),
    #[error("controller closed")]
    ControllerClosed,
    #[error("not found")]
    NotFound,
    #[error("configuration not set")]
    ConfigurationNotSet,
    #[error("used by others")]
    Used,
}

pub type Result<T = ()> = core::result::Result<T, USBError>;

impl From<xhci::ring::trb::event::CompletionCode> for USBError {
    fn from(value: xhci::ring::trb::event::CompletionCode) -> Self {
        Self::TransferEventError(value)
    }
}
