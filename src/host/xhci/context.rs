use alloc::vec::Vec;
use dma_api::{DBox, DVec};
use xhci::context::{Device, Device64Byte, Input64Byte};

use super::ring::Ring;
use crate::err::*;

pub struct DeviceContextList {
    pub dcbaa: DVec<u64>,
    pub device_context_list: Vec<DeviceContext>,
    max_slots: usize,
}

pub struct DeviceContext {
    pub out: DBox<Device64Byte>,
    pub input: DBox<Input64Byte>,
    pub transfer_rings: Vec<Ring>,
}

impl DeviceContext {
    fn new() -> Result<Self> {
        let out = DBox::zero(dma_api::Direction::ToDevice).ok_or(USBError::NoMemory)?;
        let input = DBox::zero(dma_api::Direction::FromDevice).ok_or(USBError::NoMemory)?;
        Ok(Self {
            out,
            input,
            transfer_rings: Vec::new(),
        })
    }
}

impl DeviceContextList {
    pub fn new(max_slots: usize) -> Result<Self> {
        let dcbaa =
            DVec::zeros(256, 0x1000, dma_api::Direction::ToDevice).ok_or(USBError::NoMemory)?;

        Ok(Self {
            dcbaa,
            device_context_list: Vec::new(),
            max_slots,
        })
    }

    pub fn new_slot(
        &mut self,
        slot: usize,
        num_ep: usize, // cannot lesser than 0, and consider about alignment, use usize
    ) -> Result {
        if slot > self.max_slots {
            Err(USBError::SlotLimitReached)?;
        }

        let mut ctx = DeviceContext::new()?;

        self.dcbaa.set(slot, ctx.out.bus_addr());

        ctx.transfer_rings = (0..num_ep)
            .map(|_| Ring::new_with_len(32, true, dma_api::Direction::Bidirectional))
            .try_collect()?;

        Ok(())
    }
}

pub struct ScratchpadBufferArray {
    pub entries: DVec<u64>,
    pub pages: Vec<DVec<u8>>,
}

impl ScratchpadBufferArray {
    pub fn new(entries: usize) -> Result<Self> {
        let entries =
            DVec::zeros(entries, 64, dma_api::Direction::ToDevice).ok_or(USBError::NoMemory)?;

        let pages = entries
            .iter()
            .map(|_| {
                DVec::<u8>::zeros(0x1000, 0x1000, dma_api::Direction::ToDevice)
                    .ok_or(USBError::NoMemory)
            })
            .try_collect()?;

        Ok(Self { entries, pages })
    }

    pub fn bus_addr(&self) -> u64 {
        self.entries.bus_addr()
    }
}
