use core::fmt::Display;

use libusb1_sys::constants::*;
use usb_if::err::TransferError;

#[derive(Debug, Clone, Copy)]
pub struct LibUsbErr {
    code: i32,
    msg: &'static str,
}

pub(crate) fn libusb_error_to_usb_error(code: i32) -> Result<i32, LibUsbErr> {
    if code >= LIBUSB_SUCCESS {
        return Ok(code);
    }

    let msg = match code {
        LIBUSB_ERROR_IO => "Input/output error",
        LIBUSB_ERROR_INVALID_PARAM => "Invalid parameter",
        LIBUSB_ERROR_ACCESS => "Access denied (insufficient permissions)",
        LIBUSB_ERROR_NO_DEVICE => "No such device (it may have been disconnected)",
        LIBUSB_ERROR_NOT_FOUND => "Entity not found",
        LIBUSB_ERROR_BUSY => "Resource busy",
        LIBUSB_ERROR_TIMEOUT => "Operation timed out",
        LIBUSB_ERROR_OVERFLOW => "Overflow",
        LIBUSB_ERROR_PIPE => "Pipe error",
        LIBUSB_ERROR_INTERRUPTED => "System call interrupted (perhaps due to signal)",
        LIBUSB_ERROR_NO_MEM => "Insufficient memory",
        LIBUSB_ERROR_NOT_SUPPORTED => "Operation not supported",
        _ => "Unknown error",
    };
    Err(LibUsbErr { code, msg })
}

impl Display for LibUsbErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "LibUSB error {}: {}", self.code, self.msg)
    }
}

impl core::error::Error for LibUsbErr {}

impl From<LibUsbErr> for usb_if::host::USBError {
    fn from(err: LibUsbErr) -> Self {
        match err.code {
            LIBUSB_ERROR_NOT_FOUND => usb_if::host::USBError::NotFound,
            LIBUSB_ERROR_TIMEOUT => usb_if::host::USBError::Timeout,
            LIBUSB_ERROR_NO_MEM => usb_if::host::USBError::NoMemory,
            _ => usb_if::host::USBError::Other(err.to_string()),
        }
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

macro_rules! usb {
    ($e:expr) => {
        unsafe { crate::backend::libusb::err::libusb_error_to_usb_error($e) }
    };
}
