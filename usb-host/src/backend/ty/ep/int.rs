use core::ptr::NonNull;

use usb_if::{err::TransferError, transfer::Direction};

use crate::backend::ty::{ep::TransferHandle, transfer::TransferKind};

use super::EndpointBase;

pub struct EndpointInterruptIn {
    pub(crate) raw: EndpointBase,
}

impl EndpointInterruptIn {
    pub async fn submit_and_wait(&mut self, buff: &mut [u8]) -> Result<usize, TransferError> {
        let t = self.submit(buff)?.await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub fn submit(&mut self, buff: &mut [u8]) -> Result<TransferHandle<'_>, TransferError> {
        // let transfer = Transfer::new_in(self.raw.kernel(), TransferKind::Interrupt, Pin::new(buff));
        let buff = if buff.is_empty() {
            None
        } else {
            Some((NonNull::new(buff.as_mut_ptr()).unwrap(), buff.len()))
        };

        let transfer = self
            .raw
            .new_transfer(TransferKind::Interrupt, Direction::In, buff);

        self.raw.submit(transfer)
    }
}

impl From<EndpointBase> for EndpointInterruptIn {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}

pub struct EndpointInterruptOut {
    pub(crate) raw: EndpointBase,
}

impl EndpointInterruptOut {
    pub async fn submit_and_wait(&mut self, buff: &[u8]) -> Result<usize, TransferError> {
        let t = self.submit(buff)?.await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub fn submit(&mut self, buff: &[u8]) -> Result<TransferHandle<'_>, TransferError> {
        let buff = if buff.is_empty() {
            None
        } else {
            Some((NonNull::new(buff.as_ptr() as *mut u8).unwrap(), buff.len()))
        };
        let transfer = self
            .raw
            .new_transfer(TransferKind::Interrupt, Direction::Out, buff);
        self.raw.submit(transfer)
    }
}

impl From<EndpointBase> for EndpointInterruptOut {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}
