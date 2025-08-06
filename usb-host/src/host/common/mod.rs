use alloc::{boxed::Box, vec::Vec};
use core::ptr::NonNull;

use usb_if::host::{Controller, ResultTransfer, USBError};

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
        self.raw.open().await.map(Device::from_box)
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
    raw: Box<dyn usb_if::host::Device>,
}

impl Device {
    fn from_box(raw: Box<dyn usb_if::host::Device>) -> Self {
        Device { raw }
    }

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
        self.raw
            .claim_interface(interface, alternate)
            .await
            .map(Interface::from_box)
    }

    pub async fn configuration_descriptors(
        &mut self,
    ) -> Result<Vec<usb_if::descriptor::ConfigurationDescriptor>, USBError> {
        self.raw.configuration_descriptors().await
    }
}

pub struct Interface {
    raw: Box<dyn usb_if::host::Interface>,
}

impl Interface {
    fn from_box(raw: Box<dyn usb_if::host::Interface>) -> Self {
        Interface { raw }
    }

    pub fn descriptor(&self) -> &usb_if::descriptor::InterfaceDescriptor {
        self.raw.descriptor()
    }

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
        self.raw
            .endpoint_bulk_in(endpoint)
            .map(EndpointBulkIn::from_box)
    }

    pub fn endpoint_bulk_out(&mut self, endpoint: u8) -> Result<EndpointBulkOut, USBError> {
        self.raw
            .endpoint_bulk_out(endpoint)
            .map(EndpointBulkOut::from_box)
    }

    pub fn endpoint_interrupt_in(&mut self, endpoint: u8) -> Result<EndpointInterruptIn, USBError> {
        self.raw
            .endpoint_interrupt_in(endpoint)
            .map(EndpointInterruptIn::from_box)
    }

    pub fn endpoint_interrupt_out(
        &mut self,
        endpoint: u8,
    ) -> Result<EndpointInterruptOut, USBError> {
        self.raw
            .endpoint_interrupt_out(endpoint)
            .map(EndpointInterruptOut::from_box)
    }
}

pub struct EndpointBulkIn {
    raw: Box<dyn usb_if::host::EndpointBulkIn>,
}

impl EndpointBulkIn {
    fn from_box(raw: Box<dyn usb_if::host::EndpointBulkIn>) -> Self {
        EndpointBulkIn { raw }
    }

    pub fn descriptor(&self) -> &usb_if::descriptor::EndpointDescriptor {
        self.raw.descriptor()
    }

    pub fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}

pub struct EndpointBulkOut {
    raw: Box<dyn usb_if::host::EndpointBulkOut>,
}

impl EndpointBulkOut {
    fn from_box(raw: Box<dyn usb_if::host::EndpointBulkOut>) -> Self {
        EndpointBulkOut { raw }
    }

    pub fn descriptor(&self) -> &usb_if::descriptor::EndpointDescriptor {
        self.raw.descriptor()
    }

    pub fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}

pub struct EndpointInterruptIn {
    raw: Box<dyn usb_if::host::EndpointInterruptIn>,
}

impl EndpointInterruptIn {
    fn from_box(raw: Box<dyn usb_if::host::EndpointInterruptIn>) -> Self {
        EndpointInterruptIn { raw }
    }

    pub fn descriptor(&self) -> &usb_if::descriptor::EndpointDescriptor {
        self.raw.descriptor()
    }

    pub fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}

pub struct EndpointInterruptOut {
    raw: Box<dyn usb_if::host::EndpointInterruptOut>,
}
impl EndpointInterruptOut {
    fn from_box(raw: Box<dyn usb_if::host::EndpointInterruptOut>) -> Self {
        EndpointInterruptOut { raw }
    }
    pub fn descriptor(&self) -> &usb_if::descriptor::EndpointDescriptor {
        self.raw.descriptor()
    }
    pub fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.raw.submit(data)
    }
}
