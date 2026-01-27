use core::ptr::NonNull;

use dma_api::DmaDirection;
use usb_if::transfer::Direction;

use crate::{
    backend::ty::transfer::{Transfer, TransferKind},
    osal::Kernel,
};

const ALIGN: usize = 64;

impl Transfer {
    pub(crate) fn new(
        dma: &Kernel,
        kind: TransferKind,
        direction: Direction,
        buff: Option<(NonNull<u8>, usize)>,
    ) -> Self {
        let dma_direction = match direction {
            Direction::In => DmaDirection::FromDevice,
            Direction::Out => DmaDirection::ToDevice,
        };
        let mapping = if let Some((ptr, len)) = buff {
            let slice = unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), len) };
            Some(
                dma.map_single_array(slice, ALIGN, dma_direction)
                    .expect("DMA mapping failed"),
            )
        } else {
            None
        };

        Self {
            kind,
            direction,
            mapping,
            transfer_len: 0,
        }
    }

    // pub(crate) fn new_in(dma: &Kernel, kind: TransferKind, buff: Pin<&mut [u8]>) -> Self {
    //     let buffer_addr = buff.as_ptr() as usize;
    //     let buffer_len = buff.len();
    //     trace!(
    //         "Transfer::new_in: addr={:#x}, len={}",
    //         buffer_addr, buffer_len
    //     );

    //     let mapping = if buffer_len > 0 {
    //         Some(
    //             dma.map_single_array(buff.get_mut(), ALIGN, DmaDirection::FromDevice)
    //                 .expect("DMA mapping failed"),
    //         )
    //     } else {
    //         None
    //     };

    //     Self {
    //         kind,
    //         direction: usb_if::transfer::Direction::In,
    //         mapping,
    //         transfer_len: 0,
    //     }
    // }

    // pub(crate) fn new_out(kernel: &Kernel, kind: TransferKind, buff: Pin<&[u8]>) -> Self {
    //     let buffer_addr = buff.as_ptr() as usize;
    //     let buffer_len = buff.len();
    //     trace!(
    //         "Transfer::new_out: addr={:#x}, len={}",
    //         buffer_addr, buffer_len
    //     );

    //     let mapping = if buffer_len > 0 {
    //         Some(
    //             kernel
    //                 .map_single_array(buff.get_ref(), ALIGN, DmaDirection::ToDevice)
    //                 .expect("DMA mapping failed"),
    //         )
    //     } else {
    //         None
    //     };

    //     Self {
    //         kind,
    //         direction: Direction::Out,
    //         mapping,
    //         transfer_len: 0,
    //     }
    // }

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
