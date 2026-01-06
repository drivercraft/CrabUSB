use dma_api::DVec;

use crate::BusAddr;

pub struct EventBuffer {
    pub buffer: DVec<u8>,
    pub lpos: usize,
}

impl EventBuffer {
    pub fn new(size: usize, dma_mask: usize) -> crate::err::Result<Self> {
        let buffer = DVec::zeros(dma_mask as _, size, 0x1000, dma_api::Direction::FromDevice)
            .map_err(|_| crate::err::USBError::NoMemory)?;

        Ok(Self { buffer, lpos: 0 })
    }

    pub fn dma_addr(&self) -> u64 {
        self.buffer.bus_addr()
    }
}
