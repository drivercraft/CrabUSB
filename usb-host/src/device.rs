use core::{any::type_name_of_val, fmt::Debug};

use crate::backend::ty::{DeviceInfoOp, DeviceOp, HostOp};

pub struct DeviceInfo<B: HostOp> {
    pub(crate) inner: B::DeviceInfo,
}

impl<B: HostOp> DeviceInfo<B> {
    pub fn descriptor(&self) -> &crate::DeviceDescriptor {
        self.inner.descriptor()
    }

    pub fn configurations(&self) -> &[crate::ConfigurationDescriptor] {
        self.inner.configuration_descriptors()
    }
}

impl<B: HostOp> Debug for DeviceInfo<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceInfo")
            .field("backend", &type_name_of_val(&self.inner))
            .field("vender_id", &self.inner.descriptor().vendor_id)
            .field("product_id", &self.inner.descriptor().product_id)
            .finish()
    }
}

pub struct Device<B: HostOp> {
    pub(crate) inner: <B::DeviceInfo as DeviceInfoOp>::Device,
}

impl<B> Debug for Device<B>
where
    B: HostOp,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field("backend", &type_name_of_val(&self.inner))
            .field("vender_id", &self.inner.descriptor().vendor_id)
            .field("product_id", &self.inner.descriptor().product_id)
            .finish()
    }
}

impl<B: HostOp> Device<B> {
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
}
