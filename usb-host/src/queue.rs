use alloc::sync::Arc;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::collections::BTreeMap;

use crate::BusAddr;

pub struct Finished<C> {
    inner: Arc<FinishedInner<C>>,
}

impl<C> Clone for Finished<C> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub struct FinishedInner<C> {
    finished: BTreeMap<BusAddr, AtomicBool>,
    data: UnsafeCell<BTreeMap<BusAddr, Option<C>>>,
}

unsafe impl<C> Send for FinishedInner<C> {}
unsafe impl<C> Sync for FinishedInner<C> {}

impl<C> Finished<C> {
    pub fn new(addrs: impl Iterator<Item = BusAddr>) -> Self {
        let mut finished = BTreeMap::new();
        let mut data = BTreeMap::new();

        for addr in addrs {
            finished.insert(addr, AtomicBool::new(false));
            data.insert(addr, None);
        }
        Self {
            inner: Arc::new(FinishedInner {
                data: UnsafeCell::new(data),
                finished,
            }),
        }
    }

    pub fn clear_finished(&self, addr: BusAddr) {
        if let Some(flag) = self.inner.finished.get(&addr) {
            flag.store(false, Ordering::Release);
        }
    }

    pub fn set_finished(&self, addr: BusAddr, value: C) {
        let data = unsafe { &mut *self.inner.data.get() };
        if let Some(slot) = data.get_mut(&addr) {
            *slot = Some(value);
            if let Some(flag) = self.inner.finished.get(&addr) {
                flag.store(true, Ordering::Release);
            }
        }
    }

    pub fn get_finished(&self, addr: BusAddr) -> Option<C> {
        if !self.inner.finished.get(&addr)?.load(Ordering::Acquire) {
            return None;
        }
        let data = unsafe { &mut *self.inner.data.get() };
        data.get_mut(&addr)?.take()
    }
}

