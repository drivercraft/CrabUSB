use alloc::sync::Arc;

use spin::Mutex;
use usb_if::err::TransferError;
use xhci::{registers::doorbell, ring::trb::event::TransferEvent};

use crate::{
    BusAddr,
    backend::{
        Dci,
        xhci::{SlotId, reg::SlotBell, ring::SendRing},
    },
};

pub(crate) struct EndpointRaw {
    dci: Dci,
    slot: SlotId,
    pub ring: SendRing<TransferEvent>,
    bell: Arc<Mutex<SlotBell>>,
}

unsafe impl Send for EndpointRaw {}
unsafe impl Sync for EndpointRaw {}

impl EndpointRaw {
    pub fn new(
        slot: SlotId,
        dci: Dci,
        dma_mask: usize,
        bell: Arc<Mutex<SlotBell>>,
    ) -> crate::err::Result<Self> {
        let ring = SendRing::new(dma_api::Direction::Bidirectional, dma_mask)?;

        Ok(Self {
            dci,
            slot,
            ring,
            bell,
        })
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.ring.bus_addr()
    }

    fn doorbell(&mut self) {
        let mut bell = doorbell::Register::default();
        bell.set_doorbell_target(self.dci.into());
        self.bell.lock().ring(bell);
    }

    pub fn ring(&self) -> &SendRing<TransferEvent> {
        &self.ring
    }
}
