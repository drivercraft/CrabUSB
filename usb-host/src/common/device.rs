use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::{Debug, Display};
use log::debug;
use usb_if::{
    descriptor::{
        Class, ConfigurationDescriptor, DescriptorType, InterfaceDescriptor, LanguageId,
        decode_string_descriptor,
    },
    host::{ControlSetup, USBError},
    transfer::{Recipient, Request, RequestType},
};

use crate::Interface;

pub struct Device {
    pub descriptor: usb_if::descriptor::DeviceDescriptor,
    info: Info,
    raw: Box<dyn usb_if::host::Device>,
    lang_id: LanguageId,
}

#[derive(Default)]
struct Info {
    configurations: Vec<ConfigurationDescriptor>,
    manufacturer_string: String,
    product_string: String,
    serial_number_string: String,
}

impl Device {
    pub(crate) async fn new(
        raw: Box<dyn usb_if::host::Device>,
        descriptor: usb_if::descriptor::DeviceDescriptor,
    ) -> Result<Self, USBError> {
        let mut s = Self {
            descriptor,
            info: Info::default(),
            raw,
            lang_id: LanguageId::EnglishUnitedStates,
        };
        s.lang_id = s.defautl_lang_id().await?;
        s.info.manufacturer_string = match s.descriptor.manufacturer_string_index {
            Some(index) => s.string_descriptor(index.get()).await?,
            None => String::new(),
        };

        s.info.product_string = match s.descriptor.product_string_index {
            Some(index) => s.string_descriptor(index.get()).await?,
            None => String::new(),
        };

        s.info.serial_number_string = match s.descriptor.serial_number_string_index {
            Some(index) => s.string_descriptor(index.get()).await?,
            None => String::new(),
        };

        s.init_configs().await?;
        Ok(s)
    }

    pub fn lang_id(&self) -> LanguageId {
        self.lang_id
    }

    pub fn set_lang_id(&mut self, lang_id: LanguageId) {
        self.lang_id = lang_id;
    }

    pub fn manufacturer_string(&self) -> &str {
        &self.info.manufacturer_string
    }

    pub fn product_string(&self) -> &str {
        &self.info.product_string
    }

    pub fn serial_number_string(&self) -> &str {
        &self.info.serial_number_string
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
        let mut desc = self.find_interface_desc(interface, alternate)?;
        desc.string = Some(match desc.string_index {
            Some(index) => self.string_descriptor(index.get()).await?,
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

    pub(crate) async fn init_configs(&mut self) -> Result<(), USBError> {
        if self.info.configurations.is_empty() {
            debug!("No configurations found, reading configuration descriptors");
            for i in 0..self.descriptor.num_configurations {
                let config_desc = self.read_configuration_descriptor(i).await?;
                self.info.configurations.push(config_desc);
            }
        }
        Ok(())
    }

    fn find_interface_desc(
        &self,
        interface: u8,
        alternate: u8,
    ) -> Result<InterfaceDescriptor, USBError> {
        for config in &self.info.configurations {
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

    pub fn configurations(&self) -> &[ConfigurationDescriptor] {
        &self.info.configurations
    }

    pub async fn current_configuration_descriptor(
        &mut self,
    ) -> Result<ConfigurationDescriptor, USBError> {
        let value = self.raw.get_configuration().await?;
        if value == 0 {
            return Err(USBError::NotFound);
        }
        for config in &self.info.configurations {
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

    pub async fn string_descriptor(&mut self, index: u8) -> Result<String, USBError> {
        let mut data = alloc::vec![0u8; 256];
        self.get_descriptor(
            DescriptorType::STRING,
            index,
            self.lang_id().into(),
            &mut data,
        )
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

    async fn defautl_lang_id(&mut self) -> Result<LanguageId, USBError> {
        let mut lang_buf = alloc::vec![0u8; 256];
        self.raw
            .control_in(
                ControlSetup {
                    request_type: RequestType::Standard,
                    recipient: Recipient::Device,
                    request: Request::GetDescriptor,
                    value: ((DescriptorType::STRING.0 as u16) << 8),
                    index: 0,
                },
                &mut lang_buf,
            )?
            .await?;
        if lang_buf.len() >= 4
            && (lang_buf[0] as usize) <= lang_buf.len()
            && lang_buf[1] == DescriptorType::STRING.0
        {
            let dlen = lang_buf[0] as usize;
            if dlen >= 4 {
                let langid = u16::from_le_bytes([lang_buf[2], lang_buf[3]]);
                return Ok(langid.into());
            }
        }

        Ok(LanguageId::EnglishUnitedStates) // 默认返回英语（美国）
    }
}

impl Debug for Device {
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
            .field("manufacturer_string", &self.info.manufacturer_string)
            .field("product_string", &self.info.product_string)
            .field("serial_number_string", &self.info.serial_number_string)
            .field("lang", &self.lang_id())
            .finish()
    }
}

impl Display for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} ({:04x}:{:04x})",
            self.info.product_string, self.descriptor.vendor_id, self.descriptor.product_id
        )
    }
}
