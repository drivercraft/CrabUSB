use core::{cell::UnsafeCell, sync::atomic::fence};

use alloc::{sync::Arc, vec::Vec};
use dma_api::{DBox, DVec};
use xhci::context::{Device, Device64Byte, Input32Byte, Input64Byte, InputHandler};

use super::ring::Ring;
use crate::{Slot, err::*};

pub struct DeviceContextList {
    pub dcbaa: DVec<u64>,
    pub ctx_list: Vec<Option<Arc<DeviceContext>>>,
    max_slots: usize,
}

struct ContextData {
    out: DBox<Device64Byte>,
    input: DBox<Input32Byte>,
    transfer_rings: Vec<Ring>,
}

pub struct DeviceContext {
    data: UnsafeCell<ContextData>,
}

pub struct XhciSlot {
    pub id: usize,
    ctx: Arc<DeviceContext>,
}

impl XhciSlot {
    pub fn new(slot_id: usize, ctx: Arc<DeviceContext>) -> Self {
        Self { id: slot_id, ctx }
    }

    pub fn ep_ring_ref(&self, dci: u8) -> &Ring {
        unsafe {
            let data = &*self.ctx.data.get();
            &data.transfer_rings[dci as usize - 1]
        }
    }

    pub fn modify_input(&self, f: impl FnOnce(&mut Input32Byte)) {
        unsafe {
            let data = &mut *self.ctx.data.get();
            data.input.modify(f);
        }
    }

    pub fn set_input(&self, input: Input32Byte) {
        unsafe {
            let data = &mut *self.ctx.data.get();
            data.input.write(input);
        }
    }

    pub fn input_bus_addr(&self) -> u64 {
        unsafe {
            let data = &*self.ctx.data.get();
            data.input.bus_addr()
        }
    }
}

impl Slot for XhciSlot {}

unsafe impl Send for DeviceContext {}
unsafe impl Sync for DeviceContext {}

impl ContextData {}

impl DeviceContext {
    fn new() -> Result<Self> {
        let out =
            DBox::zero_with_align(dma_api::Direction::ToDevice, 64).ok_or(USBError::NoMemory)?;
        let input =
            DBox::zero_with_align(dma_api::Direction::FromDevice, 64).ok_or(USBError::NoMemory)?;
        Ok(Self {
            data: UnsafeCell::new(ContextData {
                out,
                input,
                transfer_rings: Vec::new(),
            }),
        })
    }
}

impl DeviceContextList {
    pub fn new(max_slots: usize) -> Result<Self> {
        let dcbaa =
            DVec::zeros(256, 0x1000, dma_api::Direction::ToDevice).ok_or(USBError::NoMemory)?;

        Ok(Self {
            dcbaa,
            ctx_list: alloc::vec![ None; max_slots],
            max_slots,
        })
    }

    pub fn new_ctx(
        &mut self,
        slot_id: usize,
        num_ep: usize, // cannot lesser than 0, and consider about alignment, use usize
    ) -> Result<Arc<DeviceContext>> {
        if slot_id > self.max_slots {
            Err(USBError::SlotLimitReached)?;
        }

        let ctx = Arc::new(DeviceContext::new()?);

        let ctx_mut = unsafe { &mut *ctx.data.get() };

        self.dcbaa.set(slot_id, ctx_mut.out.bus_addr());

        ctx_mut.transfer_rings = (0..num_ep)
            .map(|_| Ring::new(true, dma_api::Direction::Bidirectional))
            .try_collect()?;

        self.ctx_list[slot_id] = Some(ctx.clone());

        Ok(ctx)
    }
}

pub struct ScratchpadBufferArray {
    pub entries: DVec<u64>,
    pub _pages: Vec<DVec<u8>>,
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

        Ok(Self {
            entries,
            _pages: pages,
        })
    }

    pub fn bus_addr(&self) -> u64 {
        self.entries.bus_addr()
    }
}
