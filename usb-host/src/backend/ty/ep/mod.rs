use core::{
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use super::transfer::Transfer;
use usb_if::err::TransferError;

mod ctrl;

pub use ctrl::*;

pub(crate) struct EndpointBase<T: EndpointOp> {
    raw: T,
}

impl<T: EndpointOp> EndpointBase<T> {
    pub fn new(raw: T) -> Self {
        Self { raw }
    }

    pub fn request(
        &mut self,
        transfer: Transfer,
    ) -> impl Future<Output = Result<Transfer, TransferError>> {
        let handle = self.raw.submit(transfer);
        async move {
            let handle = handle?;
            handle.await
        }
    }
}

impl<T: EndpointOp> Deref for EndpointBase<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.raw
    }
}

impl<T: EndpointOp> DerefMut for EndpointBase<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.raw
    }
}

pub trait EndpointOp: Send + 'static {
    fn submit(&mut self, transfer: Transfer) -> Result<TransferHandle<'_>, TransferError>;

    fn query_transfer(&mut self, id: u64) -> Option<Result<Transfer, TransferError>>;

    fn register_cx(&self, id: u64, cx: &mut Context<'_>);
}

pub struct TransferHandle<'a> {
    pub(crate) id: u64,
    pub(crate) endpoint: &'a mut dyn EndpointOp,
}

impl<'a> Future for TransferHandle<'a> {
    type Output = Result<Transfer, TransferError>;

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
