use core::ops::{Deref, DerefMut};

use alloc::boxed::Box;
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

#[repr(transparent)]
pub struct Transfer(Box<dyn TransferOp>);

impl Deref for Transfer {
    type Target = dyn TransferOp;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl DerefMut for Transfer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut()
    }
}

pub trait TransferOp: Send + 'static {
    fn transfer_len(&self) -> usize;
}
