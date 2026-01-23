use dma_api::DArray;
use mbarrier::mb;
use xhci::ring::trb::event::Allowed;

use super::ring::Ring;
use crate::{Kernel, err::*};

#[repr(C)]
pub struct EventRingSte {
    pub addr: u64,
    pub size: u16,
    _reserved: [u8; 6],
}

pub struct EventRing {
    ring: Ring,
    pub ste: DArray<EventRingSte>,
}

unsafe impl Send for EventRing {}
unsafe impl Sync for EventRing {}

impl EventRing {
    pub fn new(dma: &Kernel) -> Result<Self> {
        let ring = Ring::new(true, dma_api::Direction::Bidirectional, dma)?;

        // let mut ste = DVec::zeros(dma_mask as _, 1, 64, dma_api::Direction::Bidirectional)
        //     .map_err(|_| USBError::NoMemory)?;

        let mut ste = dma
            .new_array(1, 64, dma_api::Direction::Bidirectional)
            .map_err(|_| USBError::NoMemory)?;

        let ste0 = EventRingSte {
            addr: ring.trbs.dma_addr(),
            size: ring.len() as _,
            _reserved: [0; 6],
        };

        ste.set(0, ste0);

        Ok(Self { ring, ste })
    }

    /// 完成一次循环返回 true
    pub fn next(&mut self) -> Option<Allowed> {
        let (data, flag) = self.ring.current_data();

        let allowed = Allowed::try_from(data.to_raw()).ok()?;

        if flag != allowed.cycle_bit() {
            return None;
        }
        mb();
        self.ring.inc_deque();
        Some(allowed)
    }

    pub fn erdp(&self) -> u64 {
        self.ring.current_trb_addr().raw() & 0xFFFF_FFFF_FFFF_FFF0
    }
    pub fn erstba(&self) -> u64 {
        self.ste.dma_addr()
    }

    pub fn len(&self) -> usize {
        self.ste.len()
    }

    pub fn info(&self) -> EventRingInfo {
        EventRingInfo {
            erstz: self.len() as _,
            erdp: self.erdp(),
            erstba: self.erstba(),
        }
    }
}

pub struct EventRingInfo {
    pub erstz: u16,
    pub erdp: u64,
    pub erstba: u64,
}
