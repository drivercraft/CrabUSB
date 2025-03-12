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

    pub fn insert(&mut self, id: u64) {
        let map = unsafe { &mut *self.0.get() };
        map.insert(
            id,
            Elem {
                result: None,
                waker: AtomicWaker::new(),
            },
        );
    }

    pub fn set_result(&mut self, id: u64, result: T) {
        let entry = self.0.get_mut().get_mut(&id).unwrap();

        entry.result.replace(result);

        if let Some(wake) = entry.waker.take() {
            wake.wake();
        }
    }

    fn poll(&self, id: u64, cx: &mut core::task::Context<'_>) -> Poll<T> {
        let entry = { unsafe { &mut *self.0.get() }.get_mut(&id).unwrap() };

        match entry.result.take() {
            Some(v) => Poll::Ready(v),
            None => {
                entry.waker.register(cx.waker());
                Poll::Pending
            }
        }
    }

    pub fn wait_for_result(&self, id: u64) -> Waiter<'_, T> {
        Waiter { id, wait: self }
    }
}

pub struct Waiter<'a, T> {
    id: u64,
    wait: &'a WaitMap<T>,
}

impl<T> Future for Waiter<'_, T> {
    type Output = T;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let addr = self.id;
        self.wait.poll(addr, cx)
    }
}
