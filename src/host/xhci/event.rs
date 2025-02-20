use core::{
    cell::UnsafeCell,
    future::Future,
    sync::atomic::{AtomicBool, Ordering, fence},
    task::{Poll, Waker},
};

use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use dma_api::DVec;
use futures::{FutureExt, future::LocalBoxFuture, task::AtomicWaker};
use log::{debug, trace};
use spin::{mutex::Mutex, rwlock::RwLock};
use xhci::ring::trb::event::{Allowed, CompletionCode};

use super::ring::Ring;
use crate::err::*;

#[repr(C)]
pub struct EventRingSte {
    pub addr: u64,
    pub size: u16,
    _reserved: [u8; 6],
}

pub struct EventRing {
    pub ring: Ring,
    pub ste: DVec<EventRingSte>,
    cmd_results: UnsafeCell<BTreeMap<u64, ResultCell>>,
}

unsafe impl Send for EventRing {}
unsafe impl Sync for EventRing {}

impl EventRing {
    pub fn new(cmd_ring: &Ring) -> Result<Self> {
        let ring = Ring::new(true, dma_api::Direction::Bidirectional)?;

        let mut ste =
            DVec::zeros(1, 64, dma_api::Direction::Bidirectional).ok_or(USBError::NoMemory)?;

        let ste0 = EventRingSte {
            addr: ring.trbs.bus_addr(),
            size: ring.len() as _,
            _reserved: [0; 6],
        };

        ste.set(0, ste0);

        let mut results = BTreeMap::new();

        for i in 0..cmd_ring.len() {
            let addr = cmd_ring.trb_bus_addr(i);
            results.insert(addr, ResultCell::default());
        }

        Ok(Self {
            ring,
            ste,
            cmd_results: UnsafeCell::new(results),
        })
    }

    pub fn wait_result(&mut self, trb_addr: u64) -> LocalBoxFuture<'_, Allowed> {
        EventWaiter {
            trb_addr,
            ring: self,
        }
        .boxed_local()
    }

    pub fn clean_events(&mut self) -> usize {
        let mut count = 0;

        while let Some((allowed, _cycle)) = self.next() {
            match allowed {
                Allowed::CommandCompletion(c) => {
                    let addr = c.command_trb_pointer();
                    trace!("[EVENT] << {:?} @{:X}", allowed, addr);

                    if let Some(res) = unsafe { &mut *self.cmd_results.get() }.get_mut(&addr) {
                        res.result.replace(allowed);

                        if let Some(wake) = res.waker.take() {
                            wake.wake();
                        }
                    }
                }
                _ => {
                    debug!("unhandled event {:?}", allowed);
                }
            }
            count += 1;
        }

        count
    }

    /// 完成一次循环返回 true
    pub fn next(&mut self) -> Option<(Allowed, bool)> {
        let (data, flag) = self.ring.current_data();

        let allowed = Allowed::try_from(data.to_raw()).ok()?;

        if flag != allowed.cycle_bit() {
            return None;
        }

        fence(Ordering::SeqCst);

        let cycle = self.ring.inc_deque();
        Some((allowed, cycle))
    }

    pub fn erdp(&self) -> u64 {
        self.ring.current_trb_addr() & 0xFFFF_FFFF_FFFF_FFF0
    }
    pub fn erstba(&self) -> u64 {
        self.ste.bus_addr()
    }

    pub fn len(&self) -> usize {
        self.ste.len()
    }
}

#[derive(Default)]
struct ResultCell {
    result: Option<Allowed>,
    waker: AtomicWaker,
}

struct EventWaiter<'a> {
    trb_addr: u64,
    ring: &'a EventRing,
}

impl Future for EventWaiter<'_> {
    type Output = Allowed;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let addr = self.trb_addr;
        let entry = {
            unsafe { &mut *self.ring.cmd_results.get() }
                .get_mut(&addr)
                .unwrap()
        };

        match entry.result.take() {
            Some(v) => Poll::Ready(v),
            None => {
                entry.waker.register(cx.waker());
                Poll::Pending
            }
        }
    }
}
