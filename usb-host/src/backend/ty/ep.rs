use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use super::transfer::Transfer2;
use usb_if::err::TransferError;

pub trait EndpointOp2: Send + 'static {
    fn submit(&mut self, transfer: Transfer2) -> Result<TransferHandle2<'_>, TransferError>;

    fn query_transfer(&mut self, id: u64) -> Option<Result<Transfer2, TransferError>>;

    fn register_cx(&self, id: u64, cx: &mut Context<'_>);
}

pub struct TransferHandle2<'a> {
    pub(crate) id: u64,
    pub(crate) endpoint: &'a mut dyn EndpointOp2,
}

impl<'a> Future for TransferHandle2<'a> {
    type Output = Result<Transfer2, TransferError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let id = self.id;
        match self.endpoint.query_transfer(id) {
            Some(res) => Poll::Ready(res),
            None => {
                self.endpoint.register_cx(id, cx);
                Poll::Pending
            }
        }
    }
}
