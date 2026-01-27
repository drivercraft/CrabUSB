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

#[cfg_attr(umod, Clone)]
pub struct Transfer {
    pub kind: TransferKind,
    pub direction: usb_if::transfer::Direction,
    #[cfg(kmod)]
    pub mapping: Option<dma_api::SArrayPtr<u8>>,
    #[cfg(umod)]
    pub buffer: Option<(NonNull<u8>, usize)>,
    pub transfer_len: usize,
}
