use core::pin::Pin;

use usb_if::host::ControlSetup;

use crate::{BusAddr, backend::ty::TransferOp};

#[derive(Clone)]
pub enum TransferKind {
    Control(ControlSetup),
    // Other kinds can be added here
}

#[derive(Clone)]
pub struct Transfer {
    pub kind: TransferKind,
    pub direction: usb_if::transfer::Direction,
    pub buffer_addr: usize,
    pub buffer_len: usize,
    pub transfer_len: usize,
    pub bus_addr: BusAddr,
}

impl Transfer {
    pub fn new_in(kind: TransferKind, buff: Pin<&mut [u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_in: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );

        Self {
            kind,
            direction: usb_if::transfer::Direction::In,
            buffer_addr,
            buffer_len,
            transfer_len: 0,
            bus_addr: BusAddr(0),
        }
    }

    pub fn new_out(kind: TransferKind, buff: Pin<&[u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_out: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );
        Self {
            kind,
            direction: usb_if::transfer::Direction::Out,
            buffer_addr,
            buffer_len,
            transfer_len: 0,
            bus_addr: BusAddr(0),
        }
    }

    pub(crate) fn dma_slice<'a>(&'a self) -> dma_api::DSlice<'a, u8> {
        dma_from_usize(self.buffer_addr, self.buffer_len)
    }

    pub fn in_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.buffer_addr as *const u8, self.transfer_len) }
    }
}

fn dma_from_usize<'a>(addr: usize, len: usize) -> dma_api::DSlice<'a, u8> {
    let data_slice = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len) };
    dma_api::DSlice::from(data_slice, dma_api::Direction::Bidirectional)
}

impl TransferOp for Transfer {
    fn data_ptr(&self) -> usize {
        self.buffer_addr
    }

    fn data_len(&self) -> usize {
        self.buffer_len
    }
}
