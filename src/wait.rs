use core::{cell::UnsafeCell, task::Poll};

use alloc::collections::btree_map::BTreeMap;
use futures::task::AtomicWaker;

pub struct WaitMap<T>(UnsafeCell<BTreeMap<u64, Elem<T>>>);

unsafe impl<T> Send for WaitMap<T> {}
unsafe impl<T> Sync for WaitMap<T> {}

struct Elem<T> {
    result: Option<T>,
    waker: AtomicWaker,
}

impl<T> WaitMap<T> {
    pub fn new(id_list: &[u64]) -> Self {
        let mut map = BTreeMap::new();
        for id in id_list {
            map.insert(
                *id,
                Elem {
                    result: None,
                    waker: AtomicWaker::new(),
                },
            );
        }
        Self(UnsafeCell::new(map))
    }

    pub fn set_result(&mut self, id: u64, result: T) {
        let entry = self.0.get_mut().get_mut(&id).unwrap();

        entry.result.replace(result);

        if let Some(wake) = entry.waker.take() {
            wake.wake();
        }
    }

    pub fn poll(&self, id: u64, cx: &mut core::task::Context<'_>) -> Poll<T> {
        let entry = { unsafe { &mut *self.0.get() }.get_mut(&id).unwrap() };

        match entry.result.take() {
            Some(v) => Poll::Ready(v),
            None => {
                entry.waker.register(cx.waker());
                Poll::Pending
            }
        }
    }
}
