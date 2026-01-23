use core::{num::NonZeroUsize, pin::Pin, ptr::NonNull};

use dma_api::SingleMapping;
use usb_if::host::ControlSetup;

use crate::Kernel;

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
    pub mapping: Option<SingleMapping>,
    pub transfer_len: usize,
}

const ALIGN: usize = 64;

impl Transfer {
    pub(crate) fn new_in(dma: &Kernel, kind: TransferKind, buff: Pin<&mut [u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_in: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );

        let mapping = NonZeroUsize::new(buffer_len).map(|len| {
            dma.map_single(
                NonNull::new(buffer_addr as *mut u8).unwrap(),
                len,
                ALIGN,
                dma_api::Direction::Bidirectional,
            )
            .expect("DMA mapping failed")
        });

        Self {
            kind,
            direction: usb_if::transfer::Direction::In,
            mapping,
            transfer_len: 0,
        }
    }

    pub(crate) fn new_out(kernel: &Kernel, kind: TransferKind, buff: Pin<&[u8]>) -> Self {
        let buffer_addr = buff.as_ptr() as usize;
        let buffer_len = buff.len();
        trace!(
            "Transfer::new_out: addr={:#x}, len={}",
            buffer_addr, buffer_len
        );

        let mapping = NonZeroUsize::new(buffer_len).map(|len| {
            kernel
                .map_single(
                    NonNull::new(buffer_addr as *mut u8).unwrap(),
                    len,
                    ALIGN,
                    dma_api::Direction::ToDevice,
                )
                .expect("DMA mapping failed")
        });

        Self {
            kind,
            direction: usb_if::transfer::Direction::Out,
            mapping,
            transfer_len: 0,
        }
    }

    pub fn buffer_len(&self) -> usize {
        if let Some(ref mapping) = self.mapping {
            mapping.len()
        } else {
            0
        }
    }

    pub fn dma_addr(&self) -> u64 {
        if let Some(ref mapping) = self.mapping {
            mapping.handle.dma_addr
        } else {
            0
        }
    }

    pub fn prepare_read_all(&self) {
        if let Some(ref mapping) = self.mapping {
            mapping.prepare_read_all();
        }
    }

    pub fn confirm_write_all(&self) {
        if let Some(ref mapping) = self.mapping {
            mapping.confirm_write_all();
        }
    }
}
