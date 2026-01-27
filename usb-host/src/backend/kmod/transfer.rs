use core::pin::Pin;

use dma_api::{DmaDirection, SArrayPtr};
use usb_if::transfer::Direction;

use crate::{
    backend::ty::transfer::{TransferKind, TransferOp},
    osal::Kernel,
};

pub struct Transfer {
    pub kind: TransferKind,
    pub direction: usb_if::transfer::Direction,
    pub mapping: Option<SArrayPtr<u8>>,
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

        let mapping = if buffer_len > 0 {
            Some(
                dma.map_single_array(buff.get_mut(), ALIGN, DmaDirection::FromDevice)
                    .expect("DMA mapping failed"),
            )
        } else {
            None
        };

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

        let mapping = if buffer_len > 0 {
            Some(
                kernel
                    .map_single_array(buff.get_ref(), ALIGN, DmaDirection::ToDevice)
                    .expect("DMA mapping failed"),
            )
        } else {
            None
        };

        Self {
            kind,
            direction: Direction::Out,
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
            mapping.dma_addr().as_u64()
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

impl TransferOp for Transfer {
    fn transfer_len(&self) -> usize {
        self.transfer_len
    }
}
