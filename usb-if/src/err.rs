use alloc::{boxed::Box, string::String};

#[derive(thiserror::Error, Debug)]
pub enum TransferError {
    #[error("Stall")]
    Stall,
    #[error("Request queue full")]
    RequestQueueFull,
    #[error("Other error: {0}")]
    Other(String),
}

impl From<Box<dyn core::error::Error>> for TransferError {
    fn from(err: Box<dyn core::error::Error>) -> Self {
        TransferError::Other(alloc::format!("{err}"))
    }
}
/*

LIBUSB_SUCCESS
Success (no error)

LIBUSB_ERROR_IO
Input/output error.

LIBUSB_ERROR_INVALID_PARAM
Invalid parameter.

LIBUSB_ERROR_ACCESS
Access denied (insufficient permissions)

LIBUSB_ERROR_NO_DEVICE
No such device (it may have been disconnected)

LIBUSB_ERROR_NOT_FOUND
Entity not found.

LIBUSB_ERROR_BUSY
Resource busy.

LIBUSB_ERROR_TIMEOUT
Operation timed out.

LIBUSB_ERROR_OVERFLOW
Overflow.

LIBUSB_ERROR_PIPE
Pipe error.

LIBUSB_ERROR_INTERRUPTED
System call interrupted (perhaps due to signal)

LIBUSB_ERROR_NO_MEM
Insufficient memory.

LIBUSB_ERROR_NOT_SUPPORTED
Operation not supported or unimplemented on this platform.

LIBUSB_ERROR_OTHER
Other error.
*/
