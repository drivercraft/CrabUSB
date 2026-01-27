use alloc::boxed::Box;
use core::any::Any;
use core::ptr::NonNull;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use usb_if::transfer::Direction;

use crate::backend::ty::transfer::TransferKind;

use super::transfer::Transfer;
use usb_if::err::TransferError;

mod bulk;
mod ctrl;
mod int;
mod iso;

pub use bulk::*;
pub use ctrl::*;
pub use int::*;
pub use iso::*;

pub enum EndpointKind {
    Control(EndpointControl),
    IsochronousIn(EndpointIsoIn),
    IsochronousOut(EndpointIsoOut),
    BulkIn(EndpointBulkIn),
    BulkOut(EndpointBulkOut),
    InterruptIn(EndpointInterruptIn),
    InterruptOut(EndpointInterruptOut),
}

pub(crate) struct EndpointBase {
    raw: Box<dyn EndpointOp>,
}

impl EndpointBase {
    pub fn new(raw: impl EndpointOp) -> Self {
        Self { raw: Box::new(raw) }
    }

    pub fn new_transfer(
        &mut self,
        kind: TransferKind,
        direction: Direction,
        buff: Option<(NonNull<u8>, usize)>,
    ) -> Transfer {
        self.raw.new_transfer(kind, direction, buff)
    }

    pub fn submit_and_wait(
        &mut self,
        transfer: Transfer,
    ) -> impl Future<Output = Result<Transfer, TransferError>> {
        let handle = self.submit(transfer);
        async move {
            let handle = handle?;
            handle.await
        }
    }

    pub fn submit(&mut self, transfer: Transfer) -> Result<TransferHandle<'_>, TransferError> {
        self.raw.submit(transfer)
    }

    #[allow(unused)]
    pub(crate) fn as_raw_mut<T: EndpointOp>(&mut self) -> &mut T {
        let d = self.raw.as_mut() as &mut dyn Any;
        d.downcast_mut::<T>()
            .expect("EndpointBase downcast_mut failed")
    }
}

pub(crate) trait EndpointOp: Send + Any + 'static {
    fn new_transfer(
        &mut self,
        kind: TransferKind,
        direction: Direction,
        buff: Option<(NonNull<u8>, usize)>,
    ) -> Transfer;

    fn submit(&mut self, transfer: Transfer) -> Result<TransferHandle<'_>, TransferError>;

    fn query_transfer(&mut self, id: u64) -> Option<Result<Transfer, TransferError>>;

    fn register_cx(&self, id: u64, cx: &mut Context<'_>);
}

pub struct TransferHandle<'a> {
    pub(crate) id: u64,
    pub(crate) endpoint: &'a mut dyn EndpointOp,
}

impl<'a> TransferHandle<'a> {
    pub(crate) fn new(id: u64, endpoint: &'a mut dyn EndpointOp) -> Self {
        Self { id, endpoint }
    }
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
