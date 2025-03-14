use dma_api::DVec;
pub use dma_api::Direction;
use log::trace;
use xhci::ring::trb::{Link, command, transfer};

use crate::{err::*, page_size};

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
    pub trbs: DVec<TrbData>,
    pub i: usize,
    pub cycle: bool,
}

impl Ring {
    pub fn new_with_len(len: usize, link: bool, direction: Direction) -> Result<Self> {
        let trbs = DVec::zeros(len, 64, direction).ok_or(USBError::NoMemory)?;

        Ok(Self {
            link,
            trbs,
            i: 0,
            cycle: link,
        })
    }

    pub fn new(link: bool, direction: Direction) -> Result<Self> {
        let len = page_size() / TRB_SIZE;
        Self::new_with_len(len, link, direction)
    }

    pub fn len(&self) -> usize {
        self.trbs.len()
    }

    fn get_trb(&self) -> Option<TrbData> {
        self.trbs.get(self.i)
    }

    pub fn bus_addr(&self) -> u64 {
        self.trbs.bus_addr()
    }

    pub fn enque_command(&mut self, mut trb: command::Allowed) -> u64 {
        if self.cycle {
            trb.set_cycle_bit();
        } else {
            trb.clear_cycle_bit();
        }
        let addr = self.enque_trb(trb.into());
        trace!("[CMD] >> {:?} @{:X}", trb, addr);
        addr
    }

    pub fn enque_transfer(&mut self, mut trb: transfer::Allowed) -> u64 {
        if self.cycle {
            trb.set_cycle_bit();
        } else {
            trb.clear_cycle_bit();
        }
        let addr = self.enque_trb(trb.into());
        trace!("[Transfer] >> {:?} @{:X}", trb, addr);
        addr
    }

    pub fn enque_trb(&mut self, trb: TrbData) -> u64 {
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
            link.set_ring_segment_pointer(address).set_toggle_cycle();

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

    pub fn trb_bus_addr(&self, i: usize) -> u64 {
        let base = self.bus_addr();
        base + (i * size_of::<TrbData>()) as u64
    }

    pub fn current_trb_addr(&self) -> u64 {
        self.trb_bus_addr(self.i)
    }
}
