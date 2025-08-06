use libusb1_sys::*;

pub struct InterfaceImpl {
    raw: *mut libusb_interface,
}
