use core::{mem::MaybeUninit, num::NonZero};

use libusb1_sys::*;
use usb_if::descriptor::DeviceDescriptor;

use crate::{libusb, IDevice};

pub struct Device {
    pub(crate) raw: *mut libusb_device,
    handle: Option<*mut libusb_device_handle>,
}

unsafe impl Send for Device {}

impl Device {
    pub(crate) fn new(raw: *mut libusb_device) -> Self {
        let raw = unsafe { libusb_ref_device(raw) };
        Self { raw, handle: None }
    }

    pub fn raw(&self) -> *mut libusb_device {
        self.raw
    }

    pub fn open(&mut self) -> crate::err::Result<()> {
        let mut handle = std::ptr::null_mut();
        usb!(libusb_open(self.raw, &mut handle))?;
        self.handle = Some(handle);

        Ok(())
    }

    pub fn descriptor(&self) -> crate::err::Result<DeviceDescriptor> {
        let mut desc: MaybeUninit<libusb_device_descriptor> = MaybeUninit::uninit();
        usb!(libusb_get_device_descriptor(self.raw, desc.as_mut_ptr()))?;
        let desc = unsafe { desc.assume_init() };
        libusb_device_desc_to_desc(&desc)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            libusb_unref_device(self.raw);
        }
    }
}

impl IDevice for Device {}

fn libusb_device_desc_to_desc(
    desc: &libusb_device_descriptor,
) -> crate::err::Result<DeviceDescriptor> {
    Ok(DeviceDescriptor {
        class: desc.bDeviceClass.into(),
        subclass: desc.bDeviceSubClass.into(),
        protocol: desc.bDeviceProtocol,
        vendor_id: desc.idVendor,
        product_id: desc.idProduct,
        manufacturer_string_index: NonZero::new(desc.iManufacturer),
        product_string_index: NonZero::new(desc.iProduct),
        serial_number_string_index: NonZero::new(desc.iSerialNumber),
        num_configurations: desc.bNumConfigurations,
        usb_version: desc.bcdUSB,
        max_packet_size_0: desc.bMaxPacketSize0,
        device_version: desc.bcdDevice,
        manufacturer_string: None,
        product_string: None,
        serial_number_string: None,
    })
}
