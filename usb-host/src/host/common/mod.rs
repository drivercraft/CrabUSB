use alloc::{boxed::Box, string::String, vec::Vec};
use core::{fmt::Display, ptr::NonNull};

use usb_if::{
    descriptor::{EndpointDescriptor, InterfaceDescriptor},
    host::{Controller, ResultTransfer, USBError},
};

use crate::Xhci;

pub struct USBHost {
    raw: Box<dyn Controller>,
}

impl USBHost {
    pub fn from_trait(raw: impl Controller) -> Self {
        USBHost { raw: Box::new(raw) }
    }

    pub fn new_xhci(mmio_base: NonNull<u8>) -> Self {
        let xhci = Xhci::new(mmio_base);
        Self { raw: xhci }
    }

    pub async fn init(&mut self) -> Result<(), USBError> {
        self.raw.init().await
    }

    pub async fn device_list(&self) -> Result<impl Iterator<Item = DeviceInfo>, USBError> {
        Ok(self
            .raw
            .device_list()
            .await?
            .into_iter()
            .map(DeviceInfo::from_box))
    }

    pub fn handle_event(&mut self) {
        self.raw.handle_event();
    }
}

pub struct DeviceInfo {
    raw: Box<dyn usb_if::host::DeviceInfo>,
}

impl DeviceInfo {
    fn from_box(raw: Box<dyn usb_if::host::DeviceInfo>) -> Self {
        DeviceInfo { raw }
    }

    pub async fn open(&mut self) -> Result<Device, USBError> {
        let desc = self.raw.descriptor().await?;
        let mut device = self.raw.open().await?;
        device.set_configuration(1).await?; // Default to configuration 1
        let configurations = device.configuration_descriptors().await?;

        let manufacturer_string = match desc.manufacturer_string_index {
            Some(index) => device.string_descriptor(index.get(), 0).await?,
            None => String::new(),
        };

        let product_string = match desc.product_string_index {
            Some(index) => device.string_descriptor(index.get(), 0).await?,
            None => String::new(),
        };

        let serial_number_string = match desc.serial_number_string_index {
            Some(index) => device.string_descriptor(index.get(), 0).await?,
            None => String::new(),
        };

        Ok(Device {
            descriptor: desc,
            configurations,
            raw: device,
            manufacturer_string,
            product_string,
            serial_number_string,
        })
    }

    pub async fn descriptor(&self) -> Result<usb_if::descriptor::DeviceDescriptor, USBError> {
        self.raw.descriptor().await
    }

    pub async fn configuration_descriptors(
        &self,
    ) -> Result<Vec<usb_if::descriptor::ConfigurationDescriptor>, USBError> {
        self.raw.configuration_descriptors().await
    }
}

pub struct Device {
    pub descriptor: usb_if::descriptor::DeviceDescriptor,
    pub configurations: Vec<usb_if::descriptor::ConfigurationDescriptor>,
    pub manufacturer_string: String,
    pub product_string: String,
    pub serial_number_string: String,
    raw: Box<dyn usb_if::host::Device>,
}

impl Display for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field("manufacturer_string", &self.manufacturer_string)
            .field("product_string", &self.product_string)
            .field("serial_number_string", &self.serial_number_string)
            .finish()
    }
}

impl Device {
    pub async fn set_configuration(&mut self, configuration: u8) -> Result<(), USBError> {
        self.raw.set_configuration(configuration).await
    }

    pub async fn get_configuration(&mut self) -> Result<u8, USBError> {
        self.raw.get_configuration().await
    }

    pub async fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> Result<Interface, USBError> {
        let mut desc = self.find_interface_desc(interface, alternate)?;
        desc.string = Some(match desc.string_index {
            Some(index) => self.raw.string_descriptor(index.get(), 0).await?,
            None => String::new(),
        });
        self.raw
            .claim_interface(interface, alternate)
            .await
            .map(|raw| Interface {
                descriptor: desc,
                raw,
            })
    }

    fn find_interface_desc(
        &self,
        interface: u8,
        alternate: u8,
    ) -> Result<InterfaceDescriptor, USBError> {
        for config in &self.configurations {
            for iface in &config.interfaces {
                if iface.interface_number == interface {
                    for alt in &iface.alt_settings {
                        if alt.alternate_setting == alternate {
                            return Ok(alt.clone());
                        }
                    }
                }
            }
        }
        Err(USBError::NotFound)
    }
}

pub struct Interface {
    pub descriptor: usb_if::descriptor::InterfaceDescriptor,
    raw: Box<dyn usb_if::host::Interface>,
}

impl Display for Interface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Interface")
            .field("string", self.descriptor.string.as_ref().unwrap())
            .finish()
    }
}

impl Interface {
    pub fn control_in<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a mut [u8],
    ) -> ResultTransfer<'a> {
        self.raw.control_in(setup, data)
    }

    pub async fn control_out<'a>(
        &mut self,
        setup: usb_if::host::ControlSetup,
        data: &'a [u8],
    ) -> usb_if::host::ResultTransfer<'a> {
        self.raw.control_out(setup, data)
    }

    pub fn endpoint_bulk_in(&mut self, endpoint: u8) -> Result<EndpointBulkIn, USBError> {
        let descriptor = self.find_ep_desc(endpoint)?.clone();
        self.raw
            .endpoint_bulk_in(endpoint)
            .map(|raw| EndpointBulkIn { descriptor, raw })
    }

    pub fn endpoint_bulk_out(&mut self, endpoint: u8) -> Result<EndpointBulkOut, USBError> {
        let descriptor = self.find_ep_desc(endpoint)?.clone();
        self.raw
            .endpoint_bulk_out(endpoint)
            .map(|raw| EndpointBulkOut { descriptor, raw })
    }

    pub fn endpoint_interrupt_in(&mut self, endpoint: u8) -> Result<EndpointInterruptIn, USBError> {
        let descriptor = self.find_ep_desc(endpoint)?.clone();
        self.raw
            .endpoint_interrupt_in(endpoint)
            .map(|raw| EndpointInterruptIn { descriptor, raw })
    }

    pub fn endpoint_interrupt_out(
        &mut self,
        endpoint: u8,
    ) -> Result<EndpointInterruptOut, USBError> {
        let descriptor = self.find_ep_desc(endpoint)?.clone();
        self.raw
            .endpoint_interrupt_out(endpoint)
            .map(|raw| EndpointInterruptOut { descriptor, raw })
    }

    fn find_ep_desc(&self, address: u8) -> Result<&EndpointDescriptor, USBError> {
        self.descriptor
            .endpoints
            .iter()
            .find(|ep| ep.address == address)
            .ok_or(USBError::NotFound)
    }
}

pub struct EndpointBulkIn {
    pub descriptor: EndpointDescriptor,
    raw: Box<dyn usb_if::host::EndpointBulkIn>,
}

impl EndpointBulkIn {
    pub fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}

pub struct EndpointBulkOut {
    pub descriptor: EndpointDescriptor,
    raw: Box<dyn usb_if::host::EndpointBulkOut>,
}

impl EndpointBulkOut {
    pub fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}

pub struct EndpointInterruptIn {
    pub descriptor: EndpointDescriptor,
    raw: Box<dyn usb_if::host::EndpointInterruptIn>,
}

impl EndpointInterruptIn {
    pub fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}

pub struct EndpointInterruptOut {
    pub descriptor: EndpointDescriptor,
    raw: Box<dyn usb_if::host::EndpointInterruptOut>,
}
impl EndpointInterruptOut {
    pub fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}
