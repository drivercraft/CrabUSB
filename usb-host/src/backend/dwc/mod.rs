use alloc::vec::Vec;

use crate::{Mmio, Xhci, backend::ty::HostOp, err::Result};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

pub struct Dwc {
    xhci: Xhci,
}

impl Dwc {
    pub fn new(mmio: Mmio, dma_mask: usize) -> Result<Self> {
        let xhci = Xhci::new(mmio, dma_mask)?;

        Ok(Self { xhci })
    }
}

impl HostOp for Dwc {
    type DeviceInfo = DeviceInfo;
    type EventHandler = EventHandler;

    async fn init(&mut self) -> Result {
        self.xhci.init().await?;
        Ok(())
    }

    async fn probe_devices(&mut self) -> Result<Vec<Self::DeviceInfo>> {
        let devices = self.xhci.probe_devices().await?;
        Ok(devices)
    }

    async fn open_device(
        &mut self,
        dev: &Self::DeviceInfo,
    ) -> Result<<Self::DeviceInfo as super::ty::DeviceInfoOp>::Device> {
        let device = self.xhci.open_device(dev).await?;
        Ok(device)
    }

    fn create_event_handler(&mut self) -> Self::EventHandler {
        self.xhci.create_event_handler()
    }
}
