use alloc::vec::Vec;

use crate::Mmio;
use crate::backend::ty::*;
use crate::device::*;
use crate::err::Result;

pub use crate::backend::{
    dwc::{CruOp, Dwc, UdphyParam},
    xhci::Xhci,
};

/// USB 主机控制器
pub struct USBHost<B> {
    backend: B,
}

impl USBHost<Xhci> {
    pub fn new_xhci(mmio: Mmio, dma_mask: usize) -> Result<USBHost<Xhci>> {
        Ok(USBHost::new(Xhci::new(mmio, dma_mask)?))
    }
}

impl USBHost<Dwc> {
    pub fn new_dwc(
        ctrl: Mmio,
        phy: Mmio,
        param: UdphyParam<'_>,
        rst_list: &'_ [(&'_ str, u64)],
        cru: impl CruOp,
        dma_mask: usize,
    ) -> Result<USBHost<Dwc>> {
        Ok(USBHost::new(Dwc::new(
            ctrl, phy, param, cru, rst_list, dma_mask,
        )?))
    }
}

impl<B: HostOp> USBHost<B> {
    /// 创建新的 USB 主机控制器
    pub(crate) fn new(backend: B) -> Self {
        Self { backend }
    }

    /// 初始化主机控制器
    pub async fn init(&mut self) -> Result<()> {
        self.backend.init().await
    }

    pub async fn probe_devices(&mut self) -> Result<Vec<DeviceInfo<B>>> {
        self.backend.probe_devices().await.map(|infos| {
            infos
                .into_iter()
                .map(|info| DeviceInfo { inner: info })
                .collect()
        })
    }

    pub fn create_event_handler(&mut self) -> EventHandler<B> {
        let handler = self.backend.create_event_handler();
        EventHandler { handler }
    }

    pub async fn open_device(&mut self, dev: &DeviceInfo<B>) -> Result<Device<B>> {
        let device = self.backend.open_device(&dev.inner).await?;
        Ok(Device { inner: device })
    }
}

pub struct EventHandler<B: HostOp> {
    handler: B::EventHandler,
}

impl<B: HostOp> EventHandler<B> {
    /// 处理事件
    pub fn handle_event(&self) -> Event {
        self.handler.handle_event()
    }
}
