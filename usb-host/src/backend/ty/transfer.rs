use core::{pin::Pin, ptr::NonNull};

use dma_api::{DSliceSingle, DeviceDma, SingleMapping};
use usb_if::host::ControlSetup;

#[derive(Clone)]
pub enum TransferKind {
    Control(ControlSetup),
    Bulk,
    Interrupt,
    Isochronous { num_pkgs: usize },
}

impl TransferKind {
    pub fn get_control(&self) -> Option<&ControlSetup> {
        match self {
            TransferKind::Control(setup) => Some(setup),
            _ => None,
        }
    }
}

// #[derive(Clone)]
pub struct Transfer {
    pub kind: TransferKind,
    pub direction: usb_if::transfer::Direction,
    // pub buffer_addr: usize,
    // pub buffer_len: usize,
    pub mapping: SingleMapping,

    pub transfer_len: usize,
}

impl Transfer {
    pub fn new_in(dma: &DeviceDma, kind: TransferKind, buff: Pin<&mut [u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_in: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );

        let mapping = dma
            .map_single(
                NonNull::new(buffer_addr as *mut u8).unwrap(),
                buffer_len,
                dma_api::Direction::Bidirectional,
            )
            .expect("DMA mapping failed");

        Self {
            kind,
            direction: usb_if::transfer::Direction::In,
            mapping,
            // buffer_addr,
            // buffer_len,
            transfer_len: 0,
        }
    }

    pub fn new_out(dma: &DeviceDma, kind: TransferKind, buff: Pin<&[u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_out: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );

        let mapping = dma
            .map_single(
                NonNull::new(buffer_addr as *mut u8).unwrap(),
                buffer_len,
                dma_api::Direction::Bidirectional,
            )
            .expect("DMA mapping failed");

        Self {
            kind,
            direction: usb_if::transfer::Direction::Out,
            // buffer_addr,
            // buffer_len,
            mapping,
            transfer_len: 0,
        }
    }

    pub fn buffer_len(&self) -> usize {
        self.mapping.len()
    }


    // pub(crate) fn dma_slice<'a>(&'a self) -> dma_api::DSlice<'a, u8> {
    //     dma_from_usize(self.buffer_addr, self.buffer_len)
    // }

    // pub fn in_slice(&self) -> &[u8] {
    //     unsafe { core::slice::from_raw_parts(self.buffer_addr as *const u8, self.transfer_len) }
    // }
}

// fn dma_from_usize<'a>(addr: usize, len: usize) -> dma_api::DSliceSingle<'a, u8> {
//     let data_slice = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len) };
//     dma_api::DSlice::from(data_slice, dma_api::Direction::Bidirectional)
// }
