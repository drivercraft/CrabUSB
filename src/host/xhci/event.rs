use core::{
    cell::UnsafeCell,
    sync::atomic::{Ordering, fence},
};

use alloc::sync::{Arc, Weak};
use dma_api::DVec;
use futures::{FutureExt, future::LocalBoxFuture};
use log::{debug, trace};
use xhci::ring::trb::event::{Allowed, CommandCompletion, CompletionCode};

use super::{XhciRegisters, ring::Ring};
use crate::{
    err::*,
    wait::{WaitMap, Waiter},
};

#[repr(C)]
pub struct EventRingSte {
    pub addr: u64,
    pub size: u16,
    _reserved: [u8; 6],
}

pub struct RingWait(UnsafeCell<WaitMap<Result<CommandCompletion>>>);

unsafe impl Send for RingWait {}
unsafe impl Sync for RingWait {}

impl RingWait {
    fn new() -> Self {
        RingWait(UnsafeCell::new(WaitMap::new(&[])))
    }

    unsafe fn insert(&self, id: u64) {
        unsafe {
            (*self.0.get()).insert(id);
        }
    }

    pub(crate) fn wait_for_result(&self, id: u64) -> Waiter<'_, Result<CommandCompletion>> {
        unsafe { (*self.0.get()).wait_for_result(id) }
    }
}

pub struct EventRing {
    pub ring: Ring,
    pub ste: DVec<EventRingSte>,
    cmd_results: Arc<RingWait>,
    reg: XhciRegisters,
}

unsafe impl Send for EventRing {}
unsafe impl Sync for EventRing {}

impl EventRing {
    pub fn new(reg: XhciRegisters) -> Result<Self> {
        let ring = Ring::new(true, dma_api::Direction::Bidirectional)?;

        let mut ste =
            DVec::zeros(1, 64, dma_api::Direction::Bidirectional).ok_or(USBError::NoMemory)?;

        let ste0 = EventRingSte {
            addr: ring.trbs.bus_addr(),
            size: ring.len() as _,
            _reserved: [0; 6],
        };

        ste.set(0, ste0);

        Ok(Self {
            ring,
            ste,
            cmd_results: Arc::new(RingWait::new()),
            reg,
        })
    }

    pub fn ring_wait(&self) -> Weak<RingWait> {
        Arc::downgrade(&self.cmd_results)
    }

    pub fn listen_ring(&mut self, ring: &Ring) {
        let g = self.reg.disable_irq_guard();
        for id in (0..ring.len()).map(|i| ring.trb_bus_addr(i)) {
            unsafe { self.cmd_results.insert(id) };
        }
        drop(g);
    }

    pub fn wait_cmd_completion(
        &mut self,
        cmd_trb_addr: u64,
    ) -> LocalBoxFuture<'_, Result<CommandCompletion>> {
        self.cmd_results.wait_for_result(cmd_trb_addr).boxed_local()
    }

    pub fn clean_events(&mut self) -> usize {
        let mut count = 0;

        while let Some(allowed) = self.next() {
            match allowed {
                Allowed::CommandCompletion(c) => {
                    let addr = c.command_trb_pointer();
                    trace!("[EVENT] << {:?} @{:X}", allowed, addr);

                    let r = match c.completion_code() {
                        Ok(code) => {
                            if matches!(code, CompletionCode::Success) {
                                Ok(c)
                            } else {
                                Err(USBError::TransferEventError(code))
                            }
                        }
                        Err(_e) => Err(USBError::Unknown),
                    };

                    unsafe {
                        (*self.cmd_results.0.get()).set_result(addr, r);
                    }
                }
                Allowed::PortStatusChange(st) => {
                    debug!("port change: {}", st.port_id());
                }
                Allowed::TransferEvent(c)=>{
                    
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
    pub fn next(&mut self) -> Option<Allowed> {
        let (data, flag) = self.ring.current_data();

        let allowed = Allowed::try_from(data.to_raw()).ok()?;

        if flag != allowed.cycle_bit() {
            return None;
        }
        fence(Ordering::SeqCst);
        self.ring.inc_deque();
        Some(allowed)
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
