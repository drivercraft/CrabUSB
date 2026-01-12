use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use super::transfer::Transfer2;
use super::transfer::TransferInfo;
use usb_if::err::TransferError;

pub trait EndpointOp2: Sized + Send + 'static {
    fn submit(&mut self, transfer: Transfer2) -> Result<TransferHandle2<'_, Self>, TransferError>;

    fn query_transfer(&mut self, id: u64) -> Option<Result<TransferInfo, TransferError>>;

    fn register_cx(&self, id: u64, cx: &mut Context<'_>);
}

pub struct TransferHandle2<'a, EP: EndpointOp2> {
    pub(crate) id: u64,
    pub(crate) endpoint: &'a mut EP,
}

impl<'a, EP: EndpointOp2> Future for TransferHandle2<'a, EP> {
    type Output = Result<TransferInfo, TransferError>;

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
