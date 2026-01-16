use core::{mem::MaybeUninit, num::NonZero, ptr::null_mut};
use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use futures::FutureExt;
use libusb1_sys::*;
use usb_if::{
    descriptor::{
        ConfigurationDescriptor, DeviceDescriptor, InterfaceDescriptor, InterfaceDescriptors,
    },
    host::{ControlSetup, ResultTransfer},
    transfer::{BmRequestType, Direction},
};

use crate::backend::{
    libusb::context::Context,
    ty::{DeviceInfoOp, DeviceOp},
};
use crate::err::*;

pub struct DeviceInfo {
    pub(crate) raw: *mut libusb_device,
    desc: DeviceDescriptor,
    configs: Vec<ConfigurationDescriptor>,
    ctx: Arc<Context>,
}

impl Debug for DeviceInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceInfo").finish()
    }
}

unsafe impl Send for DeviceInfo {}
unsafe impl Sync for DeviceInfo {}

impl DeviceInfo {
    pub(crate) fn new(raw: *mut libusb_device, ctx: Arc<Context>) -> Result<Self> {
        let raw = unsafe { libusb_ref_device(raw) };
        let mut desc: MaybeUninit<libusb_device_descriptor> = MaybeUninit::uninit();
        usb!(libusb_get_device_descriptor(raw, desc.as_mut_ptr()))?;
        let desc = unsafe { desc.assume_init() };
        let desc = libusb_device_desc_to_desc(&desc)?;
        let mut configs = Vec::new();
        for i in 0..desc.num_configurations {
            let config_desc = libusb_get_configuration_descriptors(raw, i)?;
            configs.push(config_desc);
        }
        Ok(Self {
            raw,
            ctx,
            desc,
            configs,
        })
    }
}

impl Drop for DeviceInfo {
    fn drop(&mut self) {
        unsafe {
            libusb_unref_device(self.raw);
        }
    }
}

impl DeviceInfoOp for DeviceInfo {
    fn backend_name(&self) -> &str {
        "libusb"
    }

    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor] {
        &self.configs
    }
}

fn libusb_get_configuration_descriptors(
    raw: *mut libusb_device,
    index: u8,
) -> Result<ConfigurationDescriptor> {
    let mut desc: MaybeUninit<*const libusb_config_descriptor> = MaybeUninit::uninit();
    usb!(libusb_get_config_descriptor(raw, index, desc.as_mut_ptr()))?;
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
        let iface_desc = unsafe { &*desc.interface.add(iface_num) };
        let alt_setting_num = iface_desc.num_altsetting as usize;
        let mut alt_settings = Vec::with_capacity(alt_setting_num);

        for alt_idx in 0..alt_setting_num {
            let alt_desc = unsafe { &*iface_desc.altsetting.add(alt_idx) };
            let endpoint_num = alt_desc.bNumEndpoints as usize;
            let mut endpoints = Vec::with_capacity(endpoint_num);

            for ep_idx in 0..endpoint_num {
                let ep_desc = unsafe { &*alt_desc.endpoint.add(ep_idx) };
                let direction = if ep_desc.bEndpointAddress & 0x80 != 0 {
                    usb_if::transfer::Direction::In
                } else {
                    usb_if::transfer::Direction::Out
                };

                let transfer_type = match ep_desc.bmAttributes & 0x03 {
                    0 => usb_if::descriptor::EndpointType::Control,
                    1 => usb_if::descriptor::EndpointType::Isochronous,
                    2 => usb_if::descriptor::EndpointType::Bulk,
                    3 => usb_if::descriptor::EndpointType::Interrupt,
                    _ => unreachable!(),
                };

                let packets_per_microframe = match transfer_type {
                    usb_if::descriptor::EndpointType::Isochronous
                    | usb_if::descriptor::EndpointType::Interrupt => {
                        (((ep_desc.wMaxPacketSize >> 11) & 0x03) + 1) as usize
                    }
                    _ => 1,
                };

                endpoints.push(usb_if::descriptor::EndpointDescriptor {
                    address: ep_desc.bEndpointAddress & 0x0F,
                    max_packet_size: ep_desc.wMaxPacketSize & 0x7FF,
                    transfer_type,
                    direction,
                    packets_per_microframe,
                    interval: ep_desc.bInterval,
                });
            }

            alt_settings.push(InterfaceDescriptor {
                interface_number: alt_desc.bInterfaceNumber,
                alternate_setting: alt_desc.bAlternateSetting,
                class: alt_desc.bInterfaceClass,
                subclass: alt_desc.bInterfaceSubClass,
                protocol: alt_desc.bInterfaceProtocol,
                string_index: NonZero::new(alt_desc.iInterface),
                string: None,
                num_endpoints: alt_desc.bNumEndpoints,
                endpoints,
            });
        }

        interfaces.push(InterfaceDescriptors {
            interface_number: unsafe {
                if !iface_desc.altsetting.is_null() {
                    (*iface_desc.altsetting).bInterfaceNumber
                } else {
                    iface_num as u8
                }
            },
            alt_settings,
        });
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

pub struct Device {
    handle: Arc<DeviceHandle>,
}

unsafe impl Send for Device {}

impl Device {
    pub(crate) fn new(raw: *mut libusb_device_handle, ctx: Arc<Context>) -> Self {
        let handle = Arc::new(DeviceHandle { raw, _ctx: ctx });
        Self { handle }
    }
}

impl DeviceOp for Device {
    fn backend_name(&self) -> &str {
        "libusb"
    }

    fn descriptor(&self) -> &DeviceDescriptor {
        todo!()
    }

    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor] {
        todo!()
    }

    fn claim_interface<'a>(
        &'a mut self,
        interface: u8,
        alternate: u8,
    ) -> futures::future::BoxFuture<'a, std::result::Result<(), USBError>> {
        todo!()
    }

    fn ep_ctrl(&mut self) -> &mut crate::EndpointControl {
        todo!()
    }

    fn set_configuration<'a>(
        &'a mut self,
        configuration_value: u8,
    ) -> futures::future::BoxFuture<'a, std::result::Result<(), USBError>> {
        todo!()
    }

    fn get_endpoint(
        &mut self,
        desc: &usb_if::descriptor::EndpointDescriptor,
    ) -> std::result::Result<crate::backend::ty::ep::EndpointBase, USBError> {
        todo!()
    }
}

fn libusb_device_desc_to_desc(
    desc: &libusb_device_descriptor,
) -> crate::err::Result<DeviceDescriptor> {
    Ok(DeviceDescriptor {
        class: desc.bDeviceClass,
        subclass: desc.bDeviceSubClass,
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

extern "system" fn transfer_callback(_transfer: *mut libusb_transfer) {}

pub struct DeviceHandle {
    raw: *mut libusb_device_handle,
    _ctx: Arc<Context>,
}
unsafe impl Send for DeviceHandle {}
unsafe impl Sync for DeviceHandle {}

impl Drop for DeviceHandle {
    fn drop(&mut self) {
        unsafe {
            libusb_close(self.raw);
        }
    }
}

impl DeviceHandle {
    pub fn raw(&self) -> *mut libusb_device_handle {
        self.raw
    }
}
