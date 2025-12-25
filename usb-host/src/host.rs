use alloc::vec::Vec;

use crate::Mmio;
use crate::backend::ty::*;
use crate::err::Result;

pub use crate::backend::xhci::Xhci;

/// USB 主机控制器
///
/// 提供线程安全的 USB 主机控制器访问，支持事件处理
pub struct USBHost<B> {
    backend: B,
}

impl USBHost<Xhci> {
    pub fn new_xhci(mmio: Mmio, dma_mask: usize) -> Result<USBHost<Xhci>> {
        Ok(USBHost::new(Xhci::new(mmio, dma_mask)?))
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

    pub async fn probe_devices(&mut self) -> Result<Vec<B::DeviceInfo>> {
        self.backend.probe_devices().await
    }

    pub fn create_event_handler(&mut self) -> EventHandler<B> {
        let handler = self.backend.create_event_handler();
        EventHandler { handler }
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
