use alloc::{collections::BTreeMap, sync::Arc};
use xhci::ring::trb::event::TransferEvent;

use crate::{
    BusAddr,
    backend::xhci::{reg::XhciRegistersShared, ring::SendRing, sync::IrqLock},
    queue::Finished,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferId(pub(crate) BusAddr);

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
