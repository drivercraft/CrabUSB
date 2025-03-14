use core::{
    cell::UnsafeCell,
    sync::atomic::{Ordering, fence},
};

use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use dma_api::{DBox, DSliceMut, DVec, Direction};
use log::trace;
use xhci::{
    context::{Device32Byte, Input32Byte},
    registers::doorbell,
    ring::trb::transfer::{self, TransferType},
};

use super::{XhciRegisters, event::RingWait, ring::Ring};
use crate::{
    Slot,
    err::*,
    standard::trans::{self, control::ControlTransfer},
};

pub struct DeviceContextList {
    pub dcbaa: DVec<u64>,
    pub ctx_list: Vec<Option<Arc<DeviceContext>>>,
    max_slots: usize,
}

struct ContextData {
    out: DBox<Device32Byte>,
    input: DBox<Input32Byte>,
    transfer_rings: Vec<Ring>,
}

pub struct DeviceContext {
    data: UnsafeCell<ContextData>,
}

pub struct XhciSlot {
    pub id: usize,
    ctx: Arc<DeviceContext>,
    reg: XhciRegisters,
    ring_wait: Weak<RingWait>,
}

impl XhciSlot {
    pub(crate) fn new(
        slot_id: usize,
        ctx: Arc<DeviceContext>,
        reg: XhciRegisters,
        ring_wait: Weak<RingWait>,
    ) -> Self {
        Self {
            id: slot_id,
            ctx,
            reg,
            ring_wait,
        }
    }

    pub fn ep_ring_ref(&self, dci: u8) -> &Ring {
        unsafe {
            let data = &*self.ctx.data.get();
            &data.transfer_rings[dci as usize - 1]
        }
    }

    pub fn ctrl_ring_mut(&mut self) -> &mut Ring {
        unsafe {
            let data = &mut *self.ctx.data.get();
            &mut data.transfer_rings[0]
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

    pub async fn control_transfer(&mut self, urb: ControlTransfer) -> Result {
        let mut trbs: Vec<transfer::Allowed> = Vec::new();
        let mut setup = transfer::SetupStage::default();

        setup
            .set_request_type(urb.request_type.clone().into())
            .set_request(urb.request.into())
            .set_value(urb.value)
            .set_index(urb.index)
            .set_transfer_type(TransferType::No);

        let mut data = None;

        if let Some((addr, len)) = urb.data {
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) };

            let dm = DSliceMut::from(data_slice, Direction::Bidirectional);

            if matches!(urb.request_type.direction, trans::Direction::Out) {
                dm.preper_write_all();
            }

            setup
                .set_transfer_type({
                    match urb.request_type.direction {
                        trans::Direction::Out => TransferType::Out,
                        trans::Direction::In => TransferType::In,
                    }
                })
                .set_length(len);

            let mut raw_data = transfer::DataStage::default();
            raw_data
                .set_data_buffer_pointer(dm.bus_addr() as _)
                .set_trb_transfer_length(len as _)
                .set_direction(match urb.request_type.direction {
                    trans::Direction::Out => transfer::Direction::Out,
                    trans::Direction::In => transfer::Direction::In,
                });

            data = Some(raw_data)
        }

        let mut status = transfer::StatusStage::default();
        status.set_interrupt_on_completion();

        if matches!(urb.request_type.direction, trans::Direction::In) {
            status.set_direction();
        }

        trbs.push(setup.into());
        if let Some(data) = data {
            trbs.push(data.into());
        }
        trbs.push(status.into());

        let ring = self.ctrl_ring_mut();

        let mut trb_ptr = 0;

        for trb in trbs {
            trb_ptr = ring.enque_transfer(trb);
        }

        trace!("trb : {:#x}", trb_ptr);

        fence(Ordering::Release);

        let mut bell = doorbell::Register::default();
        bell.set_doorbell_target(1);

        self.reg.reg().doorbell.write_volatile_at(self.id, bell);

        let _ret = self
            .ring_wait
            .upgrade()
            .ok_or(USBError::ControllerClosed)?
            .wait_for_result(trb_ptr)
            .await?;

        if let Some((addr, len)) = urb.data {
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) };

            let dm = DSliceMut::from(data_slice, Direction::Bidirectional);

            if matches!(urb.request_type.direction, trans::Direction::In) {
                dm.preper_read_all();
            }
        }
        Ok(())
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
