use core::pin::Pin;

use alloc::{collections::BTreeMap, sync::Arc};
use usb_if::host::ControlSetup;
use xhci::ring::trb::event::TransferEvent;

use crate::{
    BusAddr,
    backend::{
        ty::TransferOp,
        xhci::{reg::XhciRegistersShared, ring::SendRing, sync::IrqLock},
    },
    queue::Finished,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferHandle(pub(crate) BusAddr);

#[derive(Clone)]
pub struct Transfer {
    pub kind: TransferKind,
    pub direction: usb_if::transfer::Direction,
    pub(crate) buffer_addr: usize,
    pub(crate) buffer_len: usize,
    pub(crate) transfer_len: usize,
    pub(crate) bus_addr: BusAddr,
}

#[derive(Clone)]
pub enum TransferKind {
    Control(ControlSetup),
    // Other kinds can be added here
}

impl Transfer {
    pub fn new_in(kind: TransferKind, buff: Pin<&mut [u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_in: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );

        Self {
            kind,
            direction: usb_if::transfer::Direction::In,
            buffer_addr,
            buffer_len,
            transfer_len: 0,
            bus_addr: 0.into(),
        }
    }

    pub fn new_out(kind: TransferKind, buff: Pin<&[u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_out: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );
        Self {
            kind,
            direction: usb_if::transfer::Direction::Out,
            buffer_addr,
            buffer_len,
            transfer_len: 0,
            bus_addr: 0.into(),
        }
    }

    pub(crate) fn dma_slice<'a>(&'a self) -> dma_api::DSlice<'a, u8> {
        dma_from_usize(self.buffer_addr, self.buffer_len)
    }

    pub fn in_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.buffer_addr as *const u8, self.transfer_len) }
    }
}

impl TransferOp for Transfer {
    fn data_ptr(&self) -> usize {
        self.buffer_addr
    }

    fn data_len(&self) -> usize {
        self.buffer_len
    }
}

fn dma_from_usize<'a>(addr: usize, len: usize) -> dma_api::DSlice<'a, u8> {
    let data_slice = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) };
    dma_api::DSlice::from(data_slice, dma_api::Direction::Bidirectional)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct TransQueueId {
    slot_id: u8,
    ep_id: u8,
}

#[derive(Clone)]
pub struct TransferResultHandler {
    inner: Arc<IrqLock<BTreeMap<TransQueueId, Finished<TransferEvent>>>>,
}

unsafe impl Send for TransferResultHandler {}

impl TransferResultHandler {
    pub fn new(reg: XhciRegistersShared) -> Self {
        Self {
            inner: Arc::new(IrqLock::new(BTreeMap::new(), reg)),
        }
    }

    pub fn register_queue(&mut self, slot_id: u8, ep_id: u8, ring: &SendRing<TransferEvent>) {
        let id = TransQueueId { slot_id, ep_id };
        let handle = ring.finished_handle();
        self.inner.lock().insert(id, handle);
    }

    pub unsafe fn set_finished(&self, slot_id: u8, ep_id: u8, ptr: BusAddr, res: TransferEvent) {
        let queue_id = TransQueueId { slot_id, ep_id };
        if let Some(q) = unsafe { self.inner.force_use().get(&queue_id) } {
            q.set_finished(ptr, res);
        }
    }
}
