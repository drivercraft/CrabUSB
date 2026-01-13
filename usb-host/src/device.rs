use alloc::boxed::Box;
use core::{any::type_name_of_val, fmt::Debug};
use usb_if::err::TransferError;
use usb_if::host::ControlSetup;

use crate::backend::BackendOp;
use crate::backend::ty::ep::EndpointControl;
use crate::backend::ty::{DeviceInfoOp, DeviceOp};

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
            .field("backend", &type_name_of_val(&self.inner))
            .field("vender_id", &self.inner.descriptor().vendor_id)
            .field("product_id", &self.inner.descriptor().product_id)
            .finish()
    }
}

pub struct Device {
    pub(crate) inner: Box<dyn DeviceOp>,
}

impl Debug for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field("backend", &type_name_of_val(&self.inner))
            .field("vender_id", &self.inner.descriptor().vendor_id)
            .field("product_id", &self.inner.descriptor().product_id)
            .finish()
    }
}

impl Device {
    pub fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> impl core::future::Future<Output = crate::err::Result<()>> + Send {
        self.inner.claim_interface(interface, alternate)
    }

    pub fn descriptor(&self) -> &crate::DeviceDescriptor {
        self.inner.descriptor()
    }

    pub fn configurations(&self) -> &[crate::ConfigurationDescriptor] {
        todo!()
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
}
