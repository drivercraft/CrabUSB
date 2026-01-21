use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::backend::{CoreOp, ty::*};
use crate::err::Result;
use crate::kcore::*;
use crate::{Mmio, backend::BackendOp};

pub use super::backend::{
    dwc::{
        CruOp, Dwc, DwcNewParams, DwcParams, UdphyParam, Usb2PhyParam, Usb2PhyPortId,
        UsbPhyInterfaceMode,
    },
    ty::ep::*,
    xhci::Xhci,
};
pub use crate::device::{Device, DeviceInfo};

/// USB 主机控制器
pub struct USBHost {
    backend: Box<dyn BackendOp>,
}

impl USBHost {
    pub fn new_xhci(mmio: Mmio, dma_mask: usize) -> Result<USBHost> {
        Ok(USBHost::new(Xhci::new(mmio, dma_mask)?))
    }

    pub fn new_dwc(params: DwcNewParams<'_, impl CruOp>) -> Result<USBHost> {
        Ok(USBHost::new(Dwc::new(params)?))
    }

    #[cfg(libusb)]
    pub fn new_libusb() -> Result<USBHost> {
        let host = USBHost::new_user(crate::backend::libusb::Libusb::new());
        Ok(host)
    }
}

impl USBHost {
    pub(crate) fn new(backend: impl CoreOp) -> Self {
        let b = Core::new(backend);
        Self {
            backend: Box::new(b),
        }
    }

    #[cfg(libusb)]
    pub(crate) fn new_user(backend: impl BackendOp) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }

    /// 初始化主机控制器
    pub async fn init(&mut self) -> Result<()> {
        self.backend.init().await?;
        Ok(())
    }

    pub async fn probe_devices(&mut self) -> Result<Vec<DeviceInfo>> {
        let device_infos = self.backend.device_list().await?;
        let mut devices = Vec::new();
        for dev in device_infos {
            let dev_info = DeviceInfo { inner: dev };
            devices.push(dev_info);
        }
        Ok(devices)
    }

    pub fn create_event_handler(&mut self) -> EventHandler {
        let handler = self.backend.create_event_handler();
        EventHandler { handler }
    }

    pub async fn open_device(&mut self, dev: &DeviceInfo) -> Result<Device> {
        let device = self.backend.open_device(dev.inner.as_ref()).await?;
        let mut device: Device = device.into();
        device.init().await?;
        Ok(device)
    }
}

pub struct EventHandler {
    handler: Box<dyn EventHandlerOp>,
}

impl EventHandler {
    /// 处理事件
    pub fn handle_event(&self) -> Event {
        self.handler.handle_event()
    }
}
