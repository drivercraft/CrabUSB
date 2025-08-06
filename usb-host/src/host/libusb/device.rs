use core::{mem::MaybeUninit, num::NonZero};

use futures::FutureExt;
use libusb1_sys::*;
use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor, InterfaceDescriptor, InterfaceDescriptors};

pub struct DeviceInfo {
    pub(crate) raw: *mut libusb_device,
}

unsafe impl Send for DeviceInfo {}

impl DeviceInfo {
    pub(crate) fn new(raw: *mut libusb_device) -> Self {
        let raw = unsafe { libusb_ref_device(raw) };
        Self { raw }
    }

    pub fn raw(&self) -> *mut libusb_device {
        self.raw
    }
}

impl Drop for DeviceInfo {
    fn drop(&mut self) {
        unsafe {
            libusb_unref_device(self.raw);
        }
    }
}

impl usb_if::host::DeviceInfo for DeviceInfo {
    fn open(
        &mut self,
    ) -> futures::future::LocalBoxFuture<
        '_,
        Result<Box<dyn usb_if::host::Device>, usb_if::host::USBError>,
    > {
        async move {
            let mut handle = std::ptr::null_mut();
            usb!(libusb_open(self.raw, &mut handle))?;
            let desc = self.descriptor().await?;
            let device = Device::new(handle, desc);

            Ok(Box::new(device) as Box<dyn usb_if::host::Device>)
        }
        .boxed_local()
    }

    fn descriptor(
        &self,
    ) -> futures::future::LocalBoxFuture<'_, Result<DeviceDescriptor, usb_if::host::USBError>> {
        async move {
            let mut desc: MaybeUninit<libusb_device_descriptor> = MaybeUninit::uninit();
            usb!(libusb_get_device_descriptor(self.raw, desc.as_mut_ptr()))?;
            let desc = unsafe { desc.assume_init() };
            libusb_device_desc_to_desc(&desc)
        }
        .boxed_local()
    }

    fn configuration_descriptor(
        &mut self,
        index: u8,
    ) -> futures::future::LocalBoxFuture<'_, Result<ConfigurationDescriptor, usb_if::host::USBError>>
    {
        async move {
            let mut desc: MaybeUninit<*const libusb_config_descriptor> = MaybeUninit::uninit();
            usb!(libusb_get_config_descriptor(
                self.raw,
                index,
                desc.as_mut_ptr()
            ))?;
            let desc = unsafe { desc.assume_init() };

            if desc.is_null() {
                return Err(usb_if::host::USBError::Other(
                    "Failed to get configuration descriptor".into(),
                ));
            }

            let desc = unsafe { &*desc };

            let interface_num = desc.bNumInterfaces as usize;
            let mut interfaces = Vec::with_capacity(interface_num);

            for iface_num in 0..interface_num {
                
            }

            let out = ConfigurationDescriptor {
                num_interfaces: desc.bNumInterfaces,
                configuration_value: desc.bConfigurationValue,
                attributes: desc.bmAttributes,
                max_power: desc.bMaxPower,
                string_index: NonZero::new(desc.iConfiguration),
                string: None,
                interfaces,
            };
            unsafe { libusb_free_config_descriptor(desc) };
            Ok(out)
        }
        .boxed_local()
    }
}

pub struct Device {
    raw: *mut libusb_device_handle,
    desc: DeviceDescriptor,
}

unsafe impl Send for Device {}

impl Device {
    pub(crate) fn new(raw: *mut libusb_device_handle, desc: DeviceDescriptor) -> Self {
        Self { raw, desc }
    }

    pub fn raw(&self) -> *mut libusb_device_handle {
        self.raw
    }

    pub fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            libusb_close(self.raw);
        }
    }
}

impl usb_if::host::Device for Device {
    fn set_configuration(
        &mut self,
        configuration: u8,
    ) -> futures::future::LocalBoxFuture<'_, Result<(), usb_if::host::USBError>> {
        todo!()
    }

    fn get_configuration(
        &mut self,
    ) -> futures::future::LocalBoxFuture<'_, Result<u8, usb_if::host::USBError>> {
        todo!()
    }

    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> futures::future::LocalBoxFuture<
        '_,
        Result<Box<dyn usb_if::host::Interface>, usb_if::host::USBError>,
    > {
        todo!()
    }

    fn string_descriptor(
        &mut self,
        index: u8,
        language_id: u16,
    ) -> futures::future::LocalBoxFuture<'_, Result<String, usb_if::host::USBError>> {
        async move {
            let mut buf = vec![0u8; 256];
            let len = usb!(libusb_get_string_descriptor_ascii(
                self.raw,
                index,
                buf.as_mut_ptr(),
                buf.len() as _
            ))?;
            buf.truncate(len as usize);
            String::from_utf8(buf).map_err(|_| {
                usb_if::host::USBError::Other("Failed to convert string descriptor to UTF-8".into())
            })
        }
        .boxed_local()
    }
}

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
    })
}
