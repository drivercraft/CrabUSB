use core::{
    fmt::Debug,
    sync::atomic::{AtomicBool, Ordering},
    task::Poll,
};

use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use futures::task::AtomicWaker;
use log::trace;

use crate::err::TransferError;

use super::sync::RwLock;

pub struct WaitMap<K: Ord + Debug, T>(Arc<RwLock<WaitMapRaw<K, T>>>);

unsafe impl<K: Ord + Debug, T> Send for WaitMap<K, T> {}
unsafe impl<K: Ord + Debug, T> Sync for WaitMap<K, T> {}

impl<K: Ord + Debug, T> WaitMap<K, T> {
    pub fn new(id_list: impl Iterator<Item = K>) -> Self {
        Self(Arc::new(RwLock::new(WaitMapRaw::new(id_list))))
    }

    pub fn empty() -> Self {
        Self(Arc::new(RwLock::new(WaitMapRaw(BTreeMap::new()))))
    }

    pub fn append(&self, id_ls: impl Iterator<Item = K>) {
        let mut raw = self.0.write();
        for id in id_ls {
            raw.0.insert(id, Elem::new());
        }
    }

    /// Sets the result for the given id.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it assumes that the id exists in the map.
    pub unsafe fn set_result(&self, id: K, result: T) {
        unsafe { self.0.force_use().set_result(id, result) };
    }

    pub fn preper_id(&self, id: &K) -> Result<(), TransferError> {
        let g = self.0.read();
        let elem =
            g.0.get(id)
                .expect("WaitMap: try_wait_for_result called with unknown id");
        if elem
            .using
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(TransferError::RequestQueueFull);
        }
        let g = unsafe { self.0.force_use() };
        let elem =
            g.0.get_mut(id)
                .expect("WaitMap: try_wait_for_result called with unknown id");
        elem.result = None;
        elem.result_ok.store(false, Ordering::Release);

        Ok(())
    }

    pub fn wait_for_result<'a>(&self, id: K, on_ready: Option<CallbackOnReady>) -> Waiter<'a, T> {
        let g = self.0.read();
        let elem =
            g.0.get(&id)
                .expect("WaitMap: try_wait_for_result called with unknown id");

        trace!("WaitMap: try_wait_for_result called with id {id:X?}, elem@{elem:p} false");
        Waiter {
            elem: elem as *const Elem<T> as *mut Elem<T>,
            _marker: core::marker::PhantomData,
            on_ready,
        }
    }
}

impl<K: Ord + Debug, T> Clone for WaitMap<K, T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct CallbackOnReady {
    pub on_ready: fn(*mut (), *mut (), *mut ()),
    pub param1: *mut (),
    pub param2: *mut (),
    pub param3: *mut (),
}

unsafe impl Send for CallbackOnReady {}

pub struct WaitMapRaw<K: Ord, T>(BTreeMap<K, Elem<T>>);

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

impl<K: Ord + Debug, T> WaitMapRaw<K, T> {
    pub fn new(id_list: impl Iterator<Item = K>) -> Self {
        let mut map = BTreeMap::new();
        for id in id_list {
            map.insert(id, Elem::new());
        }
        Self(map)
    }

    unsafe fn set_result(&mut self, id: K, result: T) {
        let entry = match self.0.get_mut(&id) {
            Some(entry) => entry,
            None => {
                let id_0 = self.0.keys().next();
                let id_end = self.0.keys().last();
                panic!(
                    "WaitMap: set_result called with unknown id {id:X?}, known ids: [{id_0:X?},{id_end:X?}]"
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

pub struct Waiter<'a, T> {
    elem: *mut Elem<T>,
    on_ready: Option<CallbackOnReady>,
    _marker: core::marker::PhantomData<&'a ()>,
}

unsafe impl<T> Send for Waiter<'_, T> {}
unsafe impl<T> Sync for Waiter<'_, T> {}

impl<T> Future for Waiter<'_, T> {
    type Output = T;

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let elem = unsafe { &mut *self.as_ref().elem };

        if elem.result_ok.load(Ordering::Acquire) {
            let result = elem
                .result
                .take()
                .unwrap_or_else(|| panic!("WaitMap: result is None when result_ok is true"));
            elem.using.store(false, Ordering::Release);
            if let Some(f) = self.as_mut().on_ready.take() {
                (f.on_ready)(f.param1, f.param2, f.param3);
            }
            return Poll::Ready(result);
        }
        elem.waker.register(cx.waker());

        Poll::Pending
    }
}
