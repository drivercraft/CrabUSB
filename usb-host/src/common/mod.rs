use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};
use core::{cell::UnsafeCell, fmt::Display, ptr::NonNull, sync::atomic::AtomicBool};
use usb_if::{
    descriptor::{Class, ConfigurationDescriptor, DeviceDescriptor, EndpointDescriptor},
    host::{Controller, ResultTransfer, USBError},
};

use crate::backend::xhci::Xhci;

mod device;
pub use device::*;

pub struct EventHandler {
    raw: Arc<HostRaw>,
    running: Arc<AtomicBool>,
}

impl EventHandler {
    pub fn handle_event(&self) -> bool {
        if !self.running.load(core::sync::atomic::Ordering::Acquire) {
            return false;
        }
        unsafe {
            (&mut *self.raw.0.get()).handle_event();
        }
        true
    }
}

struct HostRaw(UnsafeCell<Box<dyn Controller>>);

unsafe impl Sync for HostRaw {}
unsafe impl Send for HostRaw {}

impl HostRaw {
    fn new(raw: Box<dyn Controller>) -> Arc<Self> {
        Arc::new(Self(UnsafeCell::new(raw)))
    }
}

pub struct USBHost {
    raw: Arc<HostRaw>,
    running: Arc<AtomicBool>,
}

impl USBHost {
    pub fn from_trait(raw: impl Controller) -> Self {
        USBHost {
            raw: HostRaw::new(Box::new(raw)),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn new_xhci(mmio_base: NonNull<u8>) -> Self {
        let xhci = Xhci::new(mmio_base);
        Self {
            raw: HostRaw::new(xhci),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    #[cfg(feature = "libusb")]
    pub fn new_libusb() -> Self {
        let libusb = crate::backend::libusb::Libusb::new();
        Self {
            raw: HostRaw::new(Box::new(libusb)),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub async fn init(&mut self) -> Result<(), USBError> {
        self.host_mut().init().await
    }

    pub async fn device_list(&mut self) -> Result<impl Iterator<Item = DeviceInfo>, USBError> {
        let devices = self.host_mut().device_list().await?;
        let mut device_infos = Vec::with_capacity(devices.len());
        for device in devices {
            let device_info = DeviceInfo::from_box(device).await?;
            device_infos.push(device_info);
        }
        Ok(device_infos.into_iter())
    }

    fn host_mut(&mut self) -> &mut Box<dyn Controller> {
        unsafe { &mut *self.raw.0.get() }
    }

    fn handle_event(&mut self) {
        self.host_mut().handle_event();
    }

    pub fn event_handler(&mut self) -> EventHandler {
        EventHandler {
            raw: self.raw.clone(),
            running: self.running.clone(),
        }
    }
}

impl Drop for USBHost {
    fn drop(&mut self) {
        self.running
            .store(false, core::sync::atomic::Ordering::Release);
        trace!("USBHost is being dropped, stopping event handler");
    }
}

pub struct DeviceInfo {
    raw: Box<dyn usb_if::host::DeviceInfo>,
    pub descriptor: DeviceDescriptor,
    pub configurations: Vec<ConfigurationDescriptor>,
}

impl Display for DeviceInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceInfo")
            .field(
                "id",
                &alloc::format!(
                    "{:04x}:{:04x}",
                    self.descriptor.vendor_id,
                    self.descriptor.product_id
                ),
            )
            .field("class", &self.class())
            .finish()
    }
}

impl DeviceInfo {
    async fn from_box(mut raw: Box<dyn usb_if::host::DeviceInfo>) -> Result<Self, USBError> {
        let desc = raw.descriptor().await?;
        let mut configurations = Vec::with_capacity(desc.num_configurations as usize);
        for i in 0..desc.num_configurations {
            let config_desc = raw.configuration_descriptor(i).await?;

            configurations.push(config_desc);
        }
        Ok(DeviceInfo {
            raw,
            descriptor: desc,
            configurations,
        })
    }

    pub async fn open(&mut self) -> Result<Device, USBError> {
        let device = self.raw.open().await?;
        Device::new(device, self.descriptor.clone()).await
    }

    pub fn class(&self) -> Class {
        self.descriptor.class()
    }

    pub fn vendor_id(&self) -> u16 {
        self.descriptor.vendor_id
    }

    pub fn product_id(&self) -> u16 {
        self.descriptor.product_id
    }

    pub fn interface_descriptors(&self) -> Vec<usb_if::descriptor::InterfaceDescriptor> {
        let mut interfaces = BTreeMap::new();
        for config in &self.configurations {
            for iface in &config.interfaces {
                interfaces.insert(iface.interface_number, iface.first_alt_setting());
            }
        }
        interfaces.values().cloned().collect()
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
            .field("class", &self.class())
            .finish()
    }
}

impl Interface {
    pub fn set_alt_setting(&mut self, alt_setting: u8) -> Result<(), USBError> {
        self.raw.set_alt_setting(alt_setting)
    }

    pub fn class(&self) -> Class {
        self.descriptor.class()
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

    pub fn endpoint_iso_in(&mut self, endpoint: u8) -> Result<EndpointIsoIn, USBError> {
        let descriptor = self.find_ep_desc(endpoint)?.clone();
        self.raw
            .endpoint_iso_in(endpoint)
            .map(|raw| EndpointIsoIn { descriptor, raw })
    }

    pub fn endpoint_iso_out(&mut self, endpoint: u8) -> Result<EndpointIsoOut, USBError> {
        let descriptor = self.find_ep_desc(endpoint)?.clone();
        self.raw
            .endpoint_iso_out(endpoint)
            .map(|raw| EndpointIsoOut { descriptor, raw })
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

pub struct EndpointIsoIn {
    pub descriptor: EndpointDescriptor,
    raw: Box<dyn usb_if::host::EndpintIsoIn>,
}

impl EndpointIsoIn {
    pub fn submit<'a>(&mut self, data: &'a mut [u8], num_iso_packets: usize) -> ResultTransfer<'a> {
        self.raw.submit(data, num_iso_packets)
    }
}

pub struct EndpointIsoOut {
    pub descriptor: EndpointDescriptor,
    raw: Box<dyn usb_if::host::EndpintIsoOut>,
}

impl EndpointIsoOut {
    pub fn submit<'a>(&mut self, data: &'a [u8], num_iso_packets: usize) -> ResultTransfer<'a> {
        self.raw.submit(data, num_iso_packets)
    }
}
