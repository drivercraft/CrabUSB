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
            _ => Err(TransferError::Other(format!("XHCI error: {self:?}").into())),
        }
    }
}
