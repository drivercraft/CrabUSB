use libusb1_sys::*;
use usb_if::host::{EndpintIsoIn, EndpintIsoOut, Interface, USBError};

use crate::host::libusb::{device::EPControl, endpoint::EndpointImpl};

pub struct InterfaceImpl {
    raw: *mut libusb_device_handle,
    num: u8,
    alt_setting: u8,
    ctrl: EPControl,
}

impl InterfaceImpl {
    pub fn new(raw: *mut libusb_device_handle, ctrl: EPControl, num: u8, alt_setting: u8) -> Self {
        Self {
            raw,
            ctrl,
            num,
            alt_setting,
        }
    }
}

impl Drop for InterfaceImpl {
    fn drop(&mut self) {
        usb!(libusb_release_interface(self.raw, self.num as _)).unwrap();
    }
}

unsafe impl Send for InterfaceImpl {}

impl Interface for InterfaceImpl {
    fn control_in<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a mut [u8],
    ) -> usb_if::host::ResultTransfer<'a> {
        self.ctrl.control_in(setup, data)
    }

    fn control_out<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a [u8],
    ) -> usb_if::host::ResultTransfer<'a> {
        self.ctrl.control_out(setup, data)
    }

    fn endpoint_bulk_in(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointBulkIn>, usb_if::host::USBError> {
        Ok(EndpointImpl::new(self.raw, endpoint, 24))
    }

    fn endpoint_bulk_out(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointBulkOut>, usb_if::host::USBError> {
        Ok(EndpointImpl::new(self.raw, endpoint, 24))
    }

    fn endpoint_interrupt_in(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointInterruptIn>, usb_if::host::USBError> {
        Ok(EndpointImpl::new(self.raw, endpoint, 24))
    }

    fn endpoint_interrupt_out(
        &mut self,
        endpoint: u8,
    ) -> Result<Box<dyn usb_if::host::EndpointInterruptOut>, usb_if::host::USBError> {
        Ok(EndpointImpl::new(self.raw, endpoint, 24))
    }

    fn endpoint_iso_in(&mut self, endpoint: u8) -> Result<Box<dyn EndpintIsoIn>, USBError> {
        Ok(EndpointImpl::new(self.raw, endpoint, 24))
    }
    fn endpoint_iso_out(&mut self, endpoint: u8) -> Result<Box<dyn EndpintIsoOut>, USBError> {
        Ok(EndpointImpl::new(self.raw, endpoint, 24))
    }

    fn set_alt_setting(&mut self, alt_setting: u8) -> Result<(), USBError> {
        usb!(libusb_set_interface_alt_setting(
            self.raw,
            self.num as _,
            self.alt_setting as _
        ))?;
        Ok(())
    }

    fn get_alt_setting(&self) -> Result<u8, USBError> {
        todo!()
    }
}
