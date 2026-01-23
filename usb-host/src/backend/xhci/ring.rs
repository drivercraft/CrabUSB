pub use dma_api::Direction;
use dma_api::{DArray, DeviceDma};
use xhci::ring::trb::{Link, command, transfer};

use crate::{
    BusAddr, Kernel, err::*, queue::{Finished, TWaiter}
};

const TRB_LEN: usize = 4;
const TRB_SIZE: usize = size_of::<TrbData>();

#[derive(Clone)]
#[repr(transparent)]
pub struct TrbData([u32; TRB_LEN]);

impl TrbData {
    pub fn to_raw(&self) -> [u32; TRB_LEN] {
        self.0
    }
}

impl From<command::Allowed> for TrbData {
    fn from(value: command::Allowed) -> Self {
        let raw = value.into_raw();
        Self(raw)
    }
}

impl From<transfer::Allowed> for TrbData {
    fn from(value: transfer::Allowed) -> Self {
        let raw = value.into_raw();
        Self(raw)
    }
}

pub struct Ring {
    link: bool,
    pub trbs: DArray<TrbData>,
    pub i: usize,
    pub cycle: bool,
}

unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

impl Ring {
    pub fn new_with_len(
        len: usize,
        link: bool,
        direction: Direction,
        dma: &Kernel,
    ) -> core::result::Result<Self, HostError> {
        // let trbs = DArray::zeros(dma_mask as _, len, page_size(), direction)?;
        let trbs = dma.new_array(len, dma.page_size(), direction)?;

        Ok(Self {
            link,
            trbs,
            i: 0,
            cycle: link,
        })
    }

    pub fn new(link: bool, direction: Direction, dma: &Kernel) -> Result<Self> {
        let len = dma.page_size() / TRB_SIZE;
        Ok(Self::new_with_len(len, link, direction, dma)?)
    }

    pub fn len(&self) -> usize {
        self.trbs.len()
    }

    fn get_trb(&self) -> Option<TrbData> {
        self.trbs.read(self.i)
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.trbs.dma_addr().into()
    }

    pub fn enque_command(&mut self, mut trb: command::Allowed) -> BusAddr {
        if self.cycle {
            trb.set_cycle_bit();
        } else {
            trb.clear_cycle_bit();
        }
        let addr = self.enque_trb(trb.into());
        trace!("[CMD] >> {trb:X?} @{addr:X?}");
        addr
    }

    pub fn enque_transfer(&mut self, mut trb: transfer::Allowed) -> BusAddr {
        if self.cycle {
            trb.set_cycle_bit();
        } else {
            trb.clear_cycle_bit();
        }
        let addr = self.enque_trb(trb.into());
        trace!("[Transfer] >> {trb:X?} @{addr:X?}");
        addr
    }

    pub fn enque_trb(&mut self, trb: TrbData) -> BusAddr {
        self.trbs.set(self.i, trb);
        let addr = self.trb_bus_addr(self.i);
        self.next_index();
        addr
    }

    pub fn current_data(&mut self) -> (TrbData, bool) {
        (self.get_trb().unwrap(), self.cycle)
    }

    fn next_index(&mut self) -> usize {
        self.i += 1;
        let len = self.len();

        // link模式下，最后一个是Link
        if self.link && self.i >= len - 1 {
            self.i = 0;
            trace!("link!");
            let address = self.trb_bus_addr(0);
            let mut link = Link::new();
            link.set_ring_segment_pointer(address.into())
                .set_toggle_cycle();

            if self.cycle {
                link.set_cycle_bit();
            } else {
                link.clear_cycle_bit();
            }
            let trb = command::Allowed::Link(link);

            self.trbs.set(len - 1, trb.into());

            self.cycle = !self.cycle;
        } else if self.i >= len {
            self.i = 0;
        }

        self.i
    }

    pub fn inc_deque(&mut self) {
        self.i += 1;
        let len = self.len();
        if self.i >= len {
            self.i = 0;
            self.cycle = !self.cycle;
        }
    }

    pub fn trb_bus_addr(&self, i: usize) -> BusAddr {
        let base = self.bus_addr().raw();
        (base + (i * size_of::<TrbData>()) as u64).into()
    }

    pub fn current_trb_addr(&self) -> BusAddr {
        self.trb_bus_addr(self.i)
    }

    pub fn trb_bus_addr_list(&self) -> impl Iterator<Item = BusAddr> + '_ {
        (0..self.len()).map(move |i| self.trb_bus_addr(i))
    }
}

pub struct SendRing<R> {
    ring: Ring,
    finished: Finished<R>,
}

impl<R> SendRing<R> {
    pub fn new(direction: Direction, dma: &Kernel) -> Result<Self> {
        let ring = Ring::new(true, direction, dma)?;
        let finished = Finished::new(ring.trb_bus_addr_list());
        Ok(Self { ring, finished })
    }

    pub fn enque_command(&mut self, trb: command::Allowed) -> BusAddr {
        let addr = self.ring.enque_command(trb);
        self.finished.clear_finished(addr);
        addr
    }

    pub fn enque_transfer(&mut self, trb: transfer::Allowed) -> BusAddr {
        let addr = self.ring.enque_transfer(trb);
        self.finished.clear_finished(addr);
        addr
    }

    pub fn take_finished_future(&self, addr: BusAddr) -> TWaiter<R> {
        self.finished.take_waiter(addr)
    }

    pub fn finished_handle(&self) -> Finished<R> {
        self.finished.clone()
    }

    pub fn get_finished(&self, addr: BusAddr) -> Option<R> {
        self.finished.get_finished(addr)
    }

    pub fn register_cx(&self, addr: BusAddr, cx: &mut core::task::Context<'_>) {
        self.finished.register_cx(addr, cx);
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.ring.bus_addr()
    }

    pub fn cycle(&self) -> bool {
        self.ring.cycle
    }
}
