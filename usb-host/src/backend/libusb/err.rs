use core::fmt::Display;

use libusb1_sys::constants::*;
use usb_if::err::TransferError;

#[derive(Debug, Clone, Copy)]
pub struct LibUsbErr {
    code: i32,
    msg: &'static str,
}

impl Display for LibUsbErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "LibUSB error {}: {}", self.code, self.msg)
    }
}

impl core::error::Error for LibUsbErr {}

impl From<LibUsbErr> for usb_if::host::USBError {
    fn from(err: LibUsbErr) -> Self {
        usb_if::host::USBError::Other(alloc::format!("LibUSB error {}: {}", err.code, err.msg))
    }
}

pub(crate) fn transfer_status_to_result(status: i32) -> Result<(), TransferError> {
    match status {
        LIBUSB_TRANSFER_COMPLETED => Ok(()),
        LIBUSB_TRANSFER_TIMED_OUT => Err(TransferError::Timeout),
        LIBUSB_TRANSFER_CANCELLED => Err(TransferError::Cancelled),
        LIBUSB_TRANSFER_STALL => Err(TransferError::Stall),
        LIBUSB_TRANSFER_NO_DEVICE => Err(TransferError::Other("No device".into())),
        LIBUSB_TRANSFER_OVERFLOW => Err(TransferError::Other("Overflow".into())),
        _ => Err(TransferError::Other(format!(
            "Unknown transfer status: {status}"
        ))),
    }
}
