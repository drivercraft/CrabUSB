use alloc::vec::Vec;

use usb_if::host::ControlSetup;

#[derive(Clone)]
pub enum TransferKind {
    Control(ControlSetup),
    Bulk,
    Interrupt,
    Isochronous { packet_lengths: Vec<usize> },
}

impl TransferKind {
    pub fn get_control(&self) -> Option<&ControlSetup> {
        match self {
            TransferKind::Control(setup) => Some(setup),
            _ => None,
        }
    }

    pub fn iso_packet_lengths(&self) -> Option<&[usize]> {
        match self {
            TransferKind::Isochronous { packet_lengths } => Some(packet_lengths),
            _ => None,
        }
    }
}

#[cfg_attr(umod, derive(Clone))]
pub struct Transfer {
    pub kind: TransferKind,
    pub direction: usb_if::transfer::Direction,
    #[cfg(kmod)]
    pub mapping: Option<dma_api::SArrayPtr<u8>>,
    #[cfg(umod)]
    pub buffer: Option<(std::ptr::NonNull<u8>, usize)>,
    pub transfer_len: usize,
}
