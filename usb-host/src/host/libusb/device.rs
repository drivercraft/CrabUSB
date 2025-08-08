use core::{mem::MaybeUninit, num::NonZero, ptr::null_mut};
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use libusb1_sys::*;
use log::debug;
use usb_if::{
    descriptor::{
        ConfigurationDescriptor, DeviceDescriptor, InterfaceDescriptor, InterfaceDescriptors,
    },
    host::{ControlSetup, ResultTransfer},
    transfer::{BmRequestType, Direction},
};

use crate::host::libusb::{context::Context, interface::InterfaceImpl};

pub struct DeviceInfo {
    pub(crate) raw: *mut libusb_device,
    ctx: Arc<Context>,
}

unsafe impl Send for DeviceInfo {}

impl DeviceInfo {
    pub(crate) fn new(raw: *mut libusb_device, ctx: Arc<Context>) -> Self {
        let raw = unsafe { libusb_ref_device(raw) };
        Self { raw, ctx }
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
            let device = Device::new(handle, self.ctx.clone());

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
        .boxed_local()
    }
}

pub struct Device {
    handle: Arc<DeviceHandle>,
    ctrl: EPControl,
}

unsafe impl Send for Device {}

impl Device {
    pub(crate) fn new(raw: *mut libusb_device_handle, ctx: Arc<Context>) -> Self {
        let handle = Arc::new(DeviceHandle { raw, _ctx: ctx });
        let ctrl = EPControl::new(32, handle.clone());
        Self { ctrl, handle }
    }
}

impl usb_if::host::Device for Device {
    fn set_configuration(
        &mut self,
        configuration: u8,
    ) -> futures::future::LocalBoxFuture<'_, Result<(), usb_if::host::USBError>> {
        async move {
            usb!(libusb_set_configuration(
                self.handle.raw(),
                configuration as _
            ))?;
            Ok(())
        }
        .boxed_local()
    }

    fn get_configuration(
        &mut self,
    ) -> futures::future::LocalBoxFuture<'_, Result<u8, usb_if::host::USBError>> {
        async move {
            let mut config = 0;
            usb!(libusb_get_configuration(self.handle.raw(), &mut config))?;
            Ok(config as _)
        }
        .boxed_local()
    }

    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> futures::future::LocalBoxFuture<
        '_,
        Result<Box<dyn usb_if::host::Interface>, usb_if::host::USBError>,
    > {
        async move {
            let res = usb!(libusb_kernel_driver_active(
                self.handle.raw(),
                interface as _
            ))?;

            if res == 1 {
                usb!(libusb_detach_kernel_driver(
                    self.handle.raw(),
                    interface as _
                ))?;
                debug!("Kernel driver detached for interface {interface}");
            }

            usb!(libusb_claim_interface(self.handle.raw(), interface as _))?;

            debug!("Interface {interface} claimed successfully");
            if alternate != 0 {
                usb!(libusb_set_interface_alt_setting(
                    self.handle.raw(),
                    interface as _,
                    alternate as _,
                ))?;
                debug!("Interface {interface} set to alternate setting {alternate} successfully");
            }

            Ok(Box::new(InterfaceImpl::new(
                self.handle.raw(),
                self.ctrl.clone(),
                interface,
                alternate,
            )) as Box<dyn usb_if::host::Interface>)
        }
        .boxed_local()
    }

    fn string_descriptor(
        &mut self,
        index: u8,
        _language_id: u16,
    ) -> futures::future::LocalBoxFuture<'_, Result<String, usb_if::host::USBError>> {
        async move {
            let mut buf = vec![0u8; 256];
            let len = usb!(libusb_get_string_descriptor_ascii(
                self.handle.raw(),
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

    fn control_in<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a mut [u8],
    ) -> ResultTransfer<'a> {
        self.ctrl.control_in(setup, data)
    }

    fn control_out<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a [u8],
    ) -> ResultTransfer<'a> {
        self.ctrl.control_out(setup, data)
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

#[derive(Clone)]
pub struct EPControl {
    queue: Arc<Mutex<super::queue::Queue>>,
    dev: Arc<DeviceHandle>,
}

unsafe impl Send for EPControl {}
unsafe impl Sync for EPControl {}

impl EPControl {
    pub fn new(queue_size: usize, dev: Arc<DeviceHandle>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(super::queue::Queue::new(queue_size))),
            dev,
        }
    }

    pub fn control_in<'a>(&self, setup: ControlSetup, data: &'a mut [u8]) -> ResultTransfer<'a> {
        let mut queue = self.queue.lock().unwrap();
        let len = data.len();
        let dest_addr = data.as_mut_ptr() as usize;
        queue.submit(|trans, on_ready| unsafe {
            let setup_size = size_of::<libusb_control_setup>();
            trans.buff.resize(setup_size + data.len(), 0);
            let src_addr = trans.buff.as_mut_ptr();

            on_ready.param1 = src_addr as *mut ();
            on_ready.param2 = dest_addr as *mut ();
            on_ready.param3 = len as *mut ();
            on_ready.on_ready = |param1, param2, param3| {
                let src_addr = param1 as *mut u8;
                let dest_addr = param2 as *mut u8;
                let len = param3 as usize;

                std::ptr::copy_nonoverlapping(
                    src_addr.add(size_of::<libusb_control_setup>()),
                    dest_addr,
                    len,
                );
            };

            libusb_fill_control_setup(
                src_addr,
                BmRequestType::new(Direction::In, setup.request_type, setup.recipient).into(),
                setup.request.into(),
                setup.value,
                setup.index,
                data.len() as _,
            );

            libusb_fill_control_transfer(
                trans.ptr,
                self.dev.raw(),
                src_addr,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }

    pub fn control_out<'a>(&self, setup: ControlSetup, data: &'a [u8]) -> ResultTransfer<'a> {
        let mut queue = self.queue.lock().unwrap();

        queue.submit(|trans, _| unsafe {
            let setup_size = size_of::<libusb_control_setup>();
            trans.buff.resize(setup_size + data.len(), 0);
            let src_addr = trans.buff.as_mut_ptr();

            trans.buff[setup_size..].copy_from_slice(data);

            libusb_fill_control_setup(
                src_addr,
                BmRequestType::new(Direction::Out, setup.request_type, setup.recipient).into(),
                setup.request.into(),
                setup.value,
                setup.index,
                data.len() as _,
            );

            libusb_fill_control_transfer(
                trans.ptr,
                self.dev.raw(),
                src_addr,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
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
