use core::fmt::Display;

use alloc::format;
pub use usb_if::err::TransferError;
pub use usb_if::host::USBError;
use xhci::ring::trb::event::CompletionCode;

pub type Result<T = ()> = core::result::Result<T, USBError>;

pub trait ConvertXhciError {
    fn to_result(self) -> core::result::Result<(), TransferError>;
}

impl ConvertXhciError for CompletionCode {
    fn to_result(self) -> core::result::Result<(), TransferError> {
        match self {
            CompletionCode::Success => Ok(()),
            CompletionCode::ShortPacket => Ok(()),
            CompletionCode::StallError => Err(TransferError::Stall),
            CompletionCode::MissedServiceError => {
                // MissedServiceError 通常是暂时性的，可以重试
                Err(TransferError::Other(format!(
                    "XHCI temporary error: {self:?}"
                )))
            }
            _ => Err(TransferError::Other(format!("XHCI error: {self:?}"))),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub struct HostError(USBError);

impl Display for HostError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<dma_api::DError> for HostError {
    fn from(value: dma_api::DError) -> Self {
        match value {
            dma_api::DError::NoMemory => Self(USBError::NoMemory),
            dma_api::DError::DmaMaskNotMatch { mask: _, got: _ } => Self(USBError::NoMemory),
            dma_api::DError::LayoutError => Self(USBError::NoMemory),
        }
    }
}

impl From<HostError> for USBError {
    fn from(value: HostError) -> Self {
        value.0
    }
}
