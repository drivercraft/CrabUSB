use usb_if::err::TransferError;

use crate::{
    BusAddr,
    backend::{
        Dci,
        xhci::{SlotId, reg::XhciRegisters, ring::SendRing},
    },
};

pub(crate) struct EndpointRaw {
    dci: Dci,
    slot: SlotId,
    pub ring: SendRing<TransferError>,
    reg: XhciRegisters,
}

unsafe impl Send for EndpointRaw {}
unsafe impl Sync for EndpointRaw {}

impl EndpointRaw {
    pub fn new(
        slot: SlotId,
        dci: Dci,
        reg: XhciRegisters,
        dma_mask: usize,
    ) -> crate::err::Result<Self> {
        let ring = SendRing::new(dma_api::Direction::Bidirectional, dma_mask)?;

        Ok(Self {
            dci,
            slot,
            ring,
            reg,
        })
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.ring.bus_addr()
    }
}
