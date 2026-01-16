use alloc::sync::Arc;
use core::pin::Pin;
use core::task::Context;
use core::task::Poll;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};
use futures::task::AtomicWaker;

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
    data: UnsafeCell<BTreeMap<BusAddr, Arc<FinishedData<C>>>>,
}

pub struct FinishedData<C> {
    taken: AtomicBool,
    finished: AtomicBool,
    waker: AtomicWaker,
    data: UnsafeCell<Option<C>>,
}

impl<C> FinishedData<C> {
    fn new() -> Self {
        Self {
            finished: AtomicBool::new(false),
            taken: AtomicBool::new(false),
            waker: AtomicWaker::new(),
            data: UnsafeCell::new(None),
        }
    }
}

unsafe impl<C> Send for FinishedInner<C> {}
unsafe impl<C> Sync for FinishedInner<C> {}
unsafe impl<C> Send for FinishedData<C> {}
unsafe impl<C> Sync for FinishedData<C> {}

impl<C> FinishedInner<C> {
    fn clear_finished(&self, addr: BusAddr) {
        if let Some(data) = unsafe { &mut *self.data.get() }.get(&addr) {
            data.finished.store(false, Ordering::Release);
            data.taken.store(false, Ordering::Release);
            unsafe {
                (*data.data.get()).take();
            }
        }
    }
}

impl<C> Finished<C> {
    pub fn new(addrs: impl Iterator<Item = BusAddr>) -> Self {
        let mut data = BTreeMap::new();

        for addr in addrs {
            data.insert(addr, Arc::new(FinishedData::new()));
        }
        Self {
            inner: Arc::new(FinishedInner {
                data: UnsafeCell::new(data),
            }),
        }
    }

    pub fn clear_finished(&self, addr: BusAddr) {
        self.inner.clear_finished(addr);
    }

    pub fn set_finished(&self, addr: BusAddr, value: C) {
        let data = unsafe { &mut *self.inner.data.get() };
        if let Some(slot) = data.get_mut(&addr) {
            unsafe {
                *slot.data.get() = Some(value);
            }
            slot.finished.store(true, Ordering::Release);
            slot.waker.wake();
        }
    }

    pub fn get_finished(&self, addr: BusAddr) -> Option<C> {
        self.waiter(addr).get_finished()
    }

    fn waiter(&self, addr: BusAddr) -> &FinishedData<C> {
        let data = unsafe { &mut *self.inner.data.get() };
        let slot = data.get(&addr).unwrap();
        if slot.taken.load(Ordering::Acquire) {
            panic!("waiter called after take_waiter");
        }

        slot
    }

    pub fn register_cx(&self, addr: BusAddr, cx: &mut core::task::Context<'_>) {
        self.waiter(addr).register(cx.waker());
    }

    pub fn take_waiter(&self, addr: BusAddr) -> TWaiter<C> {
        let data = unsafe { &mut *self.inner.data.get() }.get(&addr).unwrap();
        if data.taken.swap(true, Ordering::AcqRel) {
            panic!("take_waiter called multiple times for the same addr");
        }
        TWaiter {
            finished: data.clone(),
        }
    }
}

pub struct TWaiter<C> {
    finished: Arc<FinishedData<C>>,
}

impl<C> Future for TWaiter<C> {
    type Output = C;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(res) = this.finished.get_finished() {
            return Poll::Ready(res);
        }
        this.finished.register(cx.waker());
        Poll::Pending
    }
}

impl<C> FinishedData<C> {
    pub fn register(&self, waker: &core::task::Waker) {
        self.waker.register(waker);
    }

    pub fn get_finished(&self) -> Option<C> {
        if !self.finished.load(Ordering::Acquire) {
            return None;
        }
        unsafe { (*self.data.get()).take() }
    }
}
