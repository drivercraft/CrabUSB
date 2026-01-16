use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::backend::ty::*;
use crate::err::Result;
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

    #[cfg(feature = "libusb")]
    pub fn new_libusb() -> Result<USBHost> {
        let host = USBHost::new(crate::backend::libusb::Libusb::new());
        Ok(host)
    }
}

impl USBHost {
    /// 创建新的 USB 主机控制器
    pub(crate) fn new(backend: impl BackendOp) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }

    /// 初始化主机控制器
    pub async fn init(&mut self) -> Result<()> {
        self.backend.init().await
    }

    pub async fn probe_devices(&mut self) -> Result<Vec<DeviceInfo>> {
        self.backend.probe_devices().await.map(|infos| {
            infos
                .into_iter()
                .map(|info| DeviceInfo { inner: info })
                .collect()
        })
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
