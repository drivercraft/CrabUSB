use alloc::{boxed::Box, string::String};
use anyhow::anyhow;
use core::{
    any::Any,
    fmt::{Debug, Display},
};

use usb_if::{
    descriptor::{
        ConfigurationDescriptor, DescriptorType, InterfaceDescriptor, LanguageId,
        decode_string_descriptor,
    },
    err::TransferError,
    host::{ControlSetup, USBError},
};

use crate::backend::ty::ep::EndpointKind;
use crate::backend::ty::{DeviceInfoOp, DeviceOp, ep::EndpointControl};

pub struct DeviceInfo {
    pub(crate) inner: Box<dyn DeviceInfoOp>,
}

impl DeviceInfo {
    pub fn descriptor(&self) -> &crate::DeviceDescriptor {
        self.inner.descriptor()
    }

    pub fn configurations(&self) -> &[crate::ConfigurationDescriptor] {
        self.inner.configuration_descriptors()
    }

    pub fn interface_descriptors<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a InterfaceDescriptor> + 'a {
        self.configurations().iter().flat_map(|config| {
            config
                .interfaces
                .iter()
                .flat_map(|interface| interface.alt_settings.first())
        })
    }

    pub fn product_id(&self) -> u16 {
        self.descriptor().product_id
    }

    pub fn vendor_id(&self) -> u16 {
        self.descriptor().vendor_id
    }
}

impl Debug for DeviceInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceInfo")
            .field("backend", &self.inner.backend_name())
            .field("vender_id", &self.inner.descriptor().vendor_id)
            .field("product_id", &self.inner.descriptor().product_id)
            .finish()
    }
}

impl Display for DeviceInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:04x}:{:04x}",
            self.inner.descriptor().vendor_id,
            self.inner.descriptor().product_id
        )
    }
}

pub struct Device {
    pub(crate) inner: Box<dyn DeviceOp>,
    lang_id: LanguageId,
    manufacturer: Option<String>,
    current_interface: Option<(u8, u8)>,
}

impl Debug for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field("backend", &self.inner.backend_name())
            .field("vender_id", &self.inner.descriptor().vendor_id)
            .field("product_id", &self.inner.descriptor().product_id)
            .finish()
    }
}

impl<T: DeviceOp> From<T> for Device {
    fn from(inner: T) -> Self {
        Self {
            inner: Box::new(inner),
            current_interface: None,
            lang_id: LanguageId::default(),
            manufacturer: None,
        }
    }
}

impl From<Box<dyn DeviceOp>> for Device {
    fn from(inner: Box<dyn DeviceOp>) -> Self {
        Self {
            inner,
            current_interface: None,
            lang_id: LanguageId::default(),
            manufacturer: None,
        }
    }
}

impl Device {
    pub(crate) async fn init(&mut self) -> Result<(), USBError> {
        self.manufacturer = self.read_manufacturer().await;
        Ok(())
    }

    pub fn product_id(&self) -> u16 {
        self.descriptor().product_id
    }

    pub fn vendor_id(&self) -> u16 {
        self.descriptor().vendor_id
    }

    pub fn slot_id(&self) -> u8 {
        self.inner.id() as _
    }

    pub async fn claim_interface(&mut self, interface: u8, alternate: u8) -> Result<(), USBError> {
        trace!("Claiming interface {interface}, alternate {alternate}");
        self.inner.claim_interface(interface, alternate).await?;
        self.current_interface = Some((interface, alternate));
        Ok(())
    }

    pub fn descriptor(&self) -> &crate::DeviceDescriptor {
        self.inner.descriptor()
    }

    pub fn configurations(&self) -> &[crate::ConfigurationDescriptor] {
        self.inner.configuration_descriptors()
    }

    pub fn manufacturer(&self) -> Option<&str> {
        self.manufacturer.as_deref()
    }

    pub async fn set_configuration(&mut self, configuration_value: u8) -> crate::err::Result {
        self.inner.set_configuration(configuration_value).await
    }

    pub fn ep_ctrl(&mut self) -> &mut EndpointControl {
        self.inner.ep_ctrl()
    }

    async fn read_manufacturer(&mut self) -> Option<String> {
        let idx = self.descriptor().manufacturer_string_index?;
        self.string_descriptor(idx.get()).await.ok()
    }

    pub fn lang_id(&self) -> LanguageId {
        self.lang_id
    }

    pub fn set_lang_id(&mut self, lang_id: LanguageId) {
        self.lang_id = lang_id;
    }

    pub async fn string_descriptor(&mut self, index: u8) -> Result<String, USBError> {
        let mut data = alloc::vec![0u8; 256];
        let lang_id = self.lang_id();
        self.ep_ctrl()
            .get_descriptor(DescriptorType::STRING, index, lang_id.into(), &mut data)
            .await?;
        let res = decode_string_descriptor(&data)?;
        Ok(res)
    }

    pub async fn control_in(
        &mut self,
        param: ControlSetup,
        buff: &mut [u8],
    ) -> Result<usize, TransferError> {
        self.ep_ctrl().control_in(param, buff).await
    }

    pub async fn control_out(
        &mut self,
        param: ControlSetup,
        buff: &[u8],
    ) -> Result<usize, TransferError> {
        self.ep_ctrl().control_out(param, buff).await
    }

    pub async fn update_hub(
        &mut self,
        params: crate::backend::ty::HubParams,
    ) -> Result<(), USBError> {
        self.inner.update_hub(params).await
    }

    pub async fn current_configuration_descriptor(
        &mut self,
    ) -> Result<ConfigurationDescriptor, USBError> {
        let value = self.ep_ctrl().get_configuration().await?;
        if value == 0 {
            return Err(USBError::NotFound);
        }
        for config in self.configurations() {
            if config.configuration_value == value {
                return Ok(config.clone());
            }
        }
        Err(USBError::NotFound)
    }

    pub async fn get_endpoint(&mut self, address: u8) -> Result<EndpointKind, USBError> {
        let ep_desc = self.find_ep_desc(address)?.clone();
        let base = self.inner.get_endpoint(&ep_desc)?;
        match (ep_desc.transfer_type, ep_desc.direction) {
            (usb_if::descriptor::EndpointType::Control, _) => {
                Ok(EndpointKind::Control(base.into()))
            }
            (usb_if::descriptor::EndpointType::Isochronous, usb_if::transfer::Direction::In) => {
                Ok(EndpointKind::IsochronousIn(base.into()))
            }
            (usb_if::descriptor::EndpointType::Isochronous, usb_if::transfer::Direction::Out) => {
                Ok(EndpointKind::IsochronousOut(base.into()))
            }
            (usb_if::descriptor::EndpointType::Bulk, usb_if::transfer::Direction::In) => {
                Ok(EndpointKind::BulkIn(base.into()))
            }
            (usb_if::descriptor::EndpointType::Bulk, usb_if::transfer::Direction::Out) => {
                Ok(EndpointKind::BulkOut(base.into()))
            }
            (usb_if::descriptor::EndpointType::Interrupt, usb_if::transfer::Direction::In) => {
                Ok(EndpointKind::InterruptIn(base.into()))
            }
            (usb_if::descriptor::EndpointType::Interrupt, usb_if::transfer::Direction::Out) => {
                Ok(EndpointKind::InterruptOut(base.into()))
            }
        }
    }

    #[allow(unused)]
    pub(crate) fn as_raw<T: DeviceOp>(&self) -> &T {
        (self.inner.as_ref() as &dyn Any)
            .downcast_ref::<T>()
            .unwrap()
    }

    #[allow(unused)]
    pub(crate) fn as_raw_mut<T: DeviceOp>(&mut self) -> &mut T {
        (self.inner.as_mut() as &mut dyn Any)
            .downcast_mut::<T>()
            .unwrap()
    }

    fn find_ep_desc(
        &self,
        address: u8,
    ) -> core::result::Result<&usb_if::descriptor::EndpointDescriptor, USBError> {
        let (interface_number, alternate_setting) = match self.current_interface {
            Some((i, a)) => (i, a),
            None => Err(anyhow!("Interface not claim"))?,
        };
        for config in self.configurations() {
            for interface in &config.interfaces {
                if interface.interface_number == interface_number {
                    for alt in &interface.alt_settings {
                        if alt.alternate_setting == alternate_setting {
                            for ep in &alt.endpoints {
                                if ep.address == address {
                                    return Ok(ep);
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(USBError::NotFound)
    }
}

impl Display for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:04x}:{:04x}",
            self.inner.descriptor().vendor_id,
            self.inner.descriptor().product_id
        )
    }
}
