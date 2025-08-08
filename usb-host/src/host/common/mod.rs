use alloc::{boxed::Box, collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::sync::atomic::AtomicBool;
use core::{fmt::Display, ptr::NonNull};
use log::{debug, trace};
use usb_if::descriptor::{Class, DescriptorType, decode_string_descriptor};
use usb_if::host::ControlSetup;
use usb_if::transfer::{Recipient, Request, RequestType};

use usb_if::{
    descriptor::{
        ConfigurationDescriptor, DeviceDescriptor, EndpointDescriptor, InterfaceDescriptor,
    },
    host::{Controller, ResultTransfer, USBError},
};

use crate::host::xhci::Xhci;

pub struct EventHandler {
    ptr: *mut USBHost,
    running: Arc<AtomicBool>,
}
unsafe impl Send for EventHandler {}
unsafe impl Sync for EventHandler {}

impl EventHandler {
    pub fn handle_event(&self) -> bool {
        if !self.running.load(core::sync::atomic::Ordering::Acquire) {
            return false;
        }
        unsafe {
            (*self.ptr).handle_event();
        }
        true
    }
}

pub struct USBHost {
    raw: Box<dyn Controller>,
    running: Arc<AtomicBool>,
}

impl USBHost {
    pub fn from_trait(raw: impl Controller) -> Self {
        USBHost {
            raw: Box::new(raw),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn new_xhci(mmio_base: NonNull<u8>) -> Self {
        let xhci = Xhci::new(mmio_base);
        Self {
            raw: xhci,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    #[cfg(feature = "libusb")]
    pub fn new_libusb() -> Self {
        let libusb = crate::host::libusb::Libusb::new();
        Self {
            raw: Box::new(libusb),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub async fn init(&mut self) -> Result<(), USBError> {
        self.raw.init().await
    }

    pub async fn device_list(&mut self) -> Result<impl Iterator<Item = DeviceInfo>, USBError> {
        let devices = self.raw.device_list().await?;
        let mut device_infos = Vec::with_capacity(devices.len());
        for device in devices {
            let device_info = DeviceInfo::from_box(device).await?;
            device_infos.push(device_info);
        }
        Ok(device_infos.into_iter())
    }

    fn handle_event(&mut self) {
        self.raw.handle_event();
    }

    pub fn event_handler(&mut self) -> EventHandler {
        EventHandler {
            ptr: self as *mut USBHost,
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
        let mut device = self.raw.open().await?;

        let manufacturer_string = match self.descriptor.manufacturer_string_index {
            Some(index) => device.string_descriptor(index.get(), 0).await?,
            None => String::new(),
        };

        let product_string = match self.descriptor.product_string_index {
            Some(index) => device.string_descriptor(index.get(), 0).await?,
            None => String::new(),
        };

        let serial_number_string = match self.descriptor.serial_number_string_index {
            Some(index) => device.string_descriptor(index.get(), 0).await?,
            None => String::new(),
        };

        // let mut config_value = device.get_configuration().await?;
        // if config_value == 0 {
        //     debug!("Setting configuration 1");
        //     device.set_configuration(1).await?; // Reset to configuration 0
        //     config_value = 1;
        // }
        // debug!("Current configuration: {config_value}");
        let mut device = Device {
            descriptor: self.descriptor.clone(),
            configurations: Vec::with_capacity(self.configurations.len()),
            raw: device,
            manufacturer_string,
            product_string,
            serial_number_string,
        };
        device.init_configs().await?;
        Ok(device)
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

pub struct Device {
    pub descriptor: usb_if::descriptor::DeviceDescriptor,
    pub configurations: Vec<ConfigurationDescriptor>,
    pub manufacturer_string: String,
    pub product_string: String,
    pub serial_number_string: String,
    raw: Box<dyn usb_if::host::Device>,
}

impl Display for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field(
                "id",
                &alloc::format!(
                    "{:04x}:{:04x}",
                    self.descriptor.vendor_id,
                    self.descriptor.product_id
                ),
            )
            .field("class", &self.class())
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

    async fn init_configs(&mut self) -> Result<(), USBError> {
        if self.configurations.is_empty() {
            debug!("No configurations found, reading configuration descriptors");
            for i in 0..self.descriptor.num_configurations {
                let config_desc = self.read_configuration_descriptor(i).await?;
                self.configurations.push(config_desc);
            }
        }
        Ok(())
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

    pub async fn current_configuration_descriptor(
        &mut self,
    ) -> Result<ConfigurationDescriptor, USBError> {
        let value = self.raw.get_configuration().await?;
        if value == 0 {
            return Err(USBError::NotFound);
        }
        for config in &self.configurations {
            if config.configuration_value == value {
                return Ok(config.clone());
            }
        }
        Err(USBError::NotFound)
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

    pub async fn string_descriptor(
        &mut self,
        index: u8,
        language_id: u16,
    ) -> Result<String, USBError> {
        // self.raw.string_descriptor(index, language_id).await
        let mut data = alloc::vec![0u8; 256];
        self.get_descriptor(DescriptorType::STRING, index, language_id, &mut data)
            .await?;
        let res = decode_string_descriptor(&data).map_err(|e| USBError::Other(e.into()))?;
        Ok(res)
    }

    async fn get_descriptor(
        &mut self,
        desc_type: DescriptorType,
        desc_index: u8,
        language_id: u16,
        buff: &mut [u8],
    ) -> Result<(), USBError> {
        self.raw
            .control_in(
                ControlSetup {
                    request_type: RequestType::Standard,
                    recipient: Recipient::Device,
                    request: Request::GetDescriptor,
                    value: ((desc_type.0 as u16) << 8) | desc_index as u16,
                    index: language_id,
                },
                buff,
            )?
            .await?;
        Ok(())
    }

    async fn read_configuration_descriptor(
        &mut self,
        index: u8,
    ) -> Result<ConfigurationDescriptor, USBError> {
        let mut header = alloc::vec![0u8; ConfigurationDescriptor::LEN]; // 配置描述符头部固定为9字节
        self.get_descriptor(DescriptorType::CONFIGURATION, index, 0, &mut header)
            .await?;

        let total_length = u16::from_le_bytes(header[2..4].try_into().unwrap()) as usize;
        // 获取完整的配置描述符（包括接口和端点描述符）
        let mut full_data = alloc::vec![0u8; total_length];
        debug!("Reading configuration descriptor for index {index}, total length: {total_length}");
        self.get_descriptor(DescriptorType::CONFIGURATION, index, 0, &mut full_data)
            .await?;
        let parsed_config = ConfigurationDescriptor::parse(&full_data)
            .ok_or(USBError::Other("config descriptor parse err".into()))?;
        Ok(parsed_config)
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
