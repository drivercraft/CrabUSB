use core::{
    sync::atomic::{AtomicBool, Ordering},
    task::Poll,
};

use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use futures::task::AtomicWaker;

use crate::sync::RwLock;

pub struct WaitMap<T>(Arc<RwLock<WaitMapRaw<T>>>);

unsafe impl<T> Send for WaitMap<T> {}
unsafe impl<T> Sync for WaitMap<T> {}

impl<T> WaitMap<T> {
    pub fn new(id_list: impl Iterator<Item = u64>) -> Self {
        Self(Arc::new(RwLock::new(WaitMapRaw::new(id_list))))
    }

    pub fn empty() -> Self {
        Self(Arc::new(RwLock::new(WaitMapRaw(BTreeMap::new()))))
    }

    pub fn append(&self, id_ls: impl Iterator<Item = u64>) {
        let mut raw = self.0.write();
        for id in id_ls {
            raw.0.insert(id, Elem::new());
        }
    }

    pub unsafe fn set_result(&self, id: u64, result: T) {
        unsafe { self.0.force_use().set_result(id, result) };
    }

    pub fn try_wait_for_result(&self, id: u64) -> Option<Waiter<T>> {
        let g = self.0.read();
        let elem =
            g.0.get(&id)
                .expect("WaitMap: try_wait_for_result called with unknown id");
        if elem
            .using
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return None;
        }
        Some(Waiter {
            elem: elem as *const Elem<T> as *mut Elem<T>,
        })
    }
}

impl<T> Clone for WaitMap<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct WaitMapRaw<T>(BTreeMap<u64, Elem<T>>);

struct Elem<T> {
    result: Option<T>,
    waker: AtomicWaker,
    using: AtomicBool,
    result_ok: AtomicBool,
}

impl<T> Elem<T> {
    fn new() -> Self {
        Self {
            result: None,
            waker: AtomicWaker::new(),
            using: AtomicBool::new(false),
            result_ok: AtomicBool::new(false),
        }
    }
}

impl<T> WaitMapRaw<T> {
    pub fn new(id_list: impl Iterator<Item = u64>) -> Self {
        let mut map = BTreeMap::new();
        for id in id_list {
            map.insert(id, Elem::new());
        }
        Self(map)
    }

    pub unsafe fn set_result(&mut self, id: u64, result: T) {
        let entry = match self.0.get_mut(&id) {
            Some(entry) => entry,
            None => {
                let id_0 = self.0.keys().next();
                let id_end = self.0.keys().last();
                panic!(
                    "WaitMap: set_result called with unknown id {id:X}, known ids: [{id_0:X?},{id_end:X?}]"
                );
            }
        };
        entry.result.replace(result);
        entry.result_ok.store(true, Ordering::Release);
        if let Some(wake) = entry.waker.take() {
            wake.wake();
        }
    }
}

pub struct Waiter<T> {
    elem: *mut Elem<T>,
}

unsafe impl<T> Send for Waiter<T> {}
unsafe impl<T> Sync for Waiter<T> {}

impl<T> Future for Waiter<T> {
    type Output = T;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let elem = unsafe { &mut *self.as_ref().elem };

        if elem.result_ok.load(Ordering::Acquire) {
            let result = elem
                .result
                .take()
                .expect("Waiter polled after result was set");
            elem.using.store(false, Ordering::Release);
            return Poll::Ready(result);
        }
        elem.waker.register(cx.waker());
        Poll::Pending
    }
}
