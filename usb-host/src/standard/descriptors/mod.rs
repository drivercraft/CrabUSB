use core::num::NonZero;

use alloc::vec::Vec;

use crate::standard::transfer::Direction;

pub(crate) mod parser;

pub struct DeviceDescriptor {
    pub usb_version: u16,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub max_packet_size_0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_version: u16,
    pub manufacturer_string_index: Option<NonZero<u8>>,
    pub product_string_index: Option<NonZero<u8>>,
    pub serial_number_string_index: Option<NonZero<u8>>,
    pub num_configurations: u8,
}

impl From<parser::DeviceDescriptor> for DeviceDescriptor {
    fn from(desc: parser::DeviceDescriptor) -> Self {
        DeviceDescriptor {
            usb_version: desc.usb_version(),
            class: desc.class(),
            subclass: desc.subclass(),
            protocol: desc.protocol(),
            max_packet_size_0: desc.max_packet_size_0(),
            vendor_id: desc.vendor_id(),
            product_id: desc.product_id(),
            device_version: desc.device_version(),
            manufacturer_string_index: desc.manufacturer_string_index(),
            product_string_index: desc.product_string_index(),
            serial_number_string_index: desc.serial_number_string_index(),
            num_configurations: desc.num_configurations(),
        }
    }
}

/// Endpoint type.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EndpointType {
    /// Control endpoint.
    Control = 0,

    /// Isochronous endpoint.
    Isochronous = 1,

    /// Bulk endpoint.
    Bulk = 2,

    /// Interrupt endpoint.
    Interrupt = 3,
}

#[derive(Debug, Clone)]
pub struct EndpointDescriptor {
    pub address: u8,
    pub max_packet_size: u16,
    pub direction: Direction,
    pub transfer_type: EndpointType,
    pub packets_per_microframe: usize,
    pub interval: u8,
}

impl EndpointDescriptor {
    pub fn dci(&self) -> u8 {
        // DCI = (endpoint_number * 2) + direction
        // Control endpoint always has DCI 1
        let endpoint_number = self.address & 0x0F; // Extract endpoint number (low 4 bits)
        (endpoint_number * 2)
            + match self.transfer_type {
                EndpointType::Control => 1, // Control endpoint always has DCI 1
                _ => {
                    if self.direction == Direction::In {
                        1
                    } else {
                        0
                    }
                }
            }
    }
}

impl From<parser::EndpointDescriptor<'_>> for EndpointDescriptor {
    fn from(desc: parser::EndpointDescriptor) -> Self {
        EndpointDescriptor {
            address: desc.address(),
            max_packet_size: desc.max_packet_size() as _,
            direction: desc.direction(),
            transfer_type: desc.transfer_type(),
            packets_per_microframe: desc.packets_per_microframe() as usize,
            interval: desc.interval(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterfaceDescriptor {
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub string_index: Option<NonZero<u8>>,
    pub num_endpoints: u8,
    pub endpoints: Vec<EndpointDescriptor>,
}

impl From<parser::InterfaceDescriptor<'_>> for InterfaceDescriptor {
    fn from(desc: parser::InterfaceDescriptor) -> Self {
        InterfaceDescriptor {
            interface_number: desc.interface_number(),
            alternate_setting: desc.alternate_setting(),
            class: desc.class(),
            subclass: desc.subclass(),
            protocol: desc.protocol(),
            string_index: desc.string_index(),
            num_endpoints: desc.num_endpoints(),
            endpoints: desc.endpoints().map(EndpointDescriptor::from).collect(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct InterfaceDescriptors {
    pub interface_number: u8,
    pub alt_settings: Vec<InterfaceDescriptor>,
}

impl InterfaceDescriptors {
    pub fn first_alt_setting(&self) -> InterfaceDescriptor {
        self.alt_settings.first().cloned().unwrap()
    }
}

impl From<parser::InterfaceDescriptors<'_>> for InterfaceDescriptors {
    fn from(desc: parser::InterfaceDescriptors) -> Self {
        InterfaceDescriptors {
            interface_number: desc.interface_number(),
            alt_settings: desc.alt_settings().map(InterfaceDescriptor::from).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigurationDescriptor {
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub attributes: u8,
    pub max_power: u8,
    pub string_index: Option<NonZero<u8>>,
    pub interfaces: Vec<InterfaceDescriptors>,
}

impl From<parser::ConfigurationDescriptor<'_>> for ConfigurationDescriptor {
    fn from(desc: parser::ConfigurationDescriptor) -> Self {
        ConfigurationDescriptor {
            num_interfaces: desc.num_interfaces(),
            configuration_value: desc.configuration_value(),
            attributes: desc.attributes(),
            max_power: desc.max_power(),
            string_index: desc.string_index(),
            interfaces: desc.interfaces().map(InterfaceDescriptors::from).collect(),
        }
    }
}
