use alloc::boxed::Box;
use alloc::vec::Vec;
use usb_if::descriptor::{Class, HubSpeed};

use crate::backend::ty::*;
use crate::err::Result;
use crate::hub::HubManager;
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
    hubs: HubManager,
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
            hubs: HubManager::new(),
        }
    }

    /// 初始化主机控制器
    pub async fn init(&mut self) -> Result<()> {
        self.backend.init().await
    }

    pub async fn probe_devices(&mut self) -> Result<Vec<DeviceInfo>> {
        let mut out = vec![];

        for dev in self.backend.probe_devices().await? {
            let info = DeviceInfo { inner: dev };
            if let Class::Hub(speed) = info.descriptor().class() {
                let mut hub_infos = self.probe_handle_hub(info, speed).await?;
                out.append(&mut hub_infos);
            } else {
                out.push(info);
            }
        }

        Ok(out)
    }

    async fn probe_handle_hub(
        &mut self,
        info: DeviceInfo,
        speed: HubSpeed,
    ) -> Result<Vec<DeviceInfo>> {
        debug!("Found hub: {:?}, speed: {:?}", info, speed);

        // TODO: 实现完整的 Hub 枚举
        // 暂时返回空设备列表，跳过 Hub 的子设备枚举
        Ok(Vec::new())
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
