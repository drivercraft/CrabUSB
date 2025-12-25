use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use xhci::ring::trb::event::TransferEvent;

use crate::{
    BusAddr,
    backend::{
        Dci,
        ty::{TransferReq, TransferRes},
        xhci::ring::SendRing,
    },
    queue::Finished,
};

pub struct TransferRequest {}

impl TransferReq for TransferRequest {}

pub struct TransferResult {}

impl TransferRes for TransferResult {}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct TransQueueId {
    slot_id: u8,
    ep_id: u8,
}

pub struct TransferResultHandler {
    inner: BTreeMap<TransQueueId, Finished<TransferEvent>>,
}

unsafe impl Send for TransferResultHandler {}

impl TransferResultHandler {
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    pub fn register_queue(&mut self, slot_id: u8, ep_id: u8, ring: &SendRing<TransferEvent>) {
        let id = TransQueueId { slot_id, ep_id };
        self.inner.insert(id, ring.finished_handle());
    }

    pub fn set_finished(&self, slot_id: u8, ep_id: u8, ptr: BusAddr, res: TransferEvent) {
        let queue_id = TransQueueId { slot_id, ep_id };
        if let Some(q) = self.inner.get(&queue_id) {
            q.set_finished(ptr, res);
        }
    }
}
