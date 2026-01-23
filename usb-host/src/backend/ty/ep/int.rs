use core::pin::Pin;

use usb_if::err::TransferError;

use crate::{
    TransferHandle,
    backend::ty::transfer::{Transfer, TransferKind},
};

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
        let transfer = Transfer::new_in(self.raw.kernel(), TransferKind::Interrupt, Pin::new(buff));
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
        let transfer =
            Transfer::new_out(self.raw.kernel(), TransferKind::Interrupt, Pin::new(buff));
        self.raw.submit(transfer)
    }
}

impl From<EndpointBase> for EndpointInterruptOut {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}
