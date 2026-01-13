use alloc::boxed::Box;
use core::any::Any;
use core::fmt::Debug;
use usb_if::{
    descriptor::ConfigurationDescriptor,
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

pub struct Device {
    pub(crate) inner: Box<dyn DeviceOp>,
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
        }
    }
}

impl From<Box<dyn DeviceOp>> for Device {
    fn from(inner: Box<dyn DeviceOp>) -> Self {
        Self {
            inner,
            current_interface: None,
        }
    }
}

impl Device {
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

    pub async fn set_configuration(&mut self, configuration_value: u8) -> crate::err::Result {
        self.inner.set_configuration(configuration_value).await
    }

    pub fn ep_ctrl(&mut self) -> &mut EndpointControl {
        self.inner.ep_ctrl()
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
        let base = self.inner.get_endpoint(&ep_desc).await?;
        match ep_desc.transfer_type {
            usb_if::descriptor::EndpointType::Control => Ok(EndpointKind::Control(
                crate::backend::ty::ep::EndpointControl::new_from_base(base),
            )),
            usb_if::descriptor::EndpointType::Isochronous => Ok(EndpointKind::Isochronous),
            usb_if::descriptor::EndpointType::Bulk => Ok(EndpointKind::Bulk),
            usb_if::descriptor::EndpointType::Interrupt => Ok(EndpointKind::Interrupt),
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
            None => return Err(USBError::Other("Interface not claim".into())),
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
