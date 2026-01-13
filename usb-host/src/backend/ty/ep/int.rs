use core::pin::Pin;

use usb_if::err::TransferError;

use crate::backend::ty::transfer::{Transfer, TransferKind};

use super::EndpointBase;

pub struct EndpointInterrupt {
    pub(crate) raw: EndpointBase,
}

impl EndpointInterrupt {
    pub async fn transfer_in(&mut self, buff: &mut [u8]) -> Result<usize, TransferError> {
        let transfer = Transfer::new_in(TransferKind::Interrupt, Pin::new(buff));
        let t = self.raw.request(transfer).await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub async fn transfer_out(&mut self, buff: &[u8]) -> Result<usize, TransferError> {
        let transfer = Transfer::new_out(TransferKind::Interrupt, Pin::new(buff));
        let t = self.raw.request(transfer).await?;
        let n = t.transfer_len;
        Ok(n)
    }
}

impl From<EndpointBase> for EndpointInterrupt {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}
