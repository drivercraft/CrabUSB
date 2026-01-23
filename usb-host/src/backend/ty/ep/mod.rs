use alloc::boxed::Box;
use core::any::Any;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::Kernel;

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

impl EndpointKind {
    // pub(crate) fn as_raw_mut<T: EndpointOp>(&mut self) -> &mut T {
    //     match self {
    //         EndpointKind::Control(ep) => ep.raw.as_raw_mut::<T>(),
    //         EndpointKind::Isochronous => {
    //             panic!("EndpointType::as_type_mut: Isochronous endpoint not implemented")
    //         }
    //         EndpointKind::Bulk => {
    //             panic!("EndpointType::as_type_mut: Bulk endpoint not implemented")
    //         }
    //         EndpointKind::Interrupt => {
    //             panic!("EndpointType::as_type_mut: Interrupt endpoint not implemented")
    //         }
    //     }
    // }

    // pub(crate) fn as_raw_ref<T: EndpointOp>(&self) -> &T {
    //     match self {
    //         EndpointKind::Control(ep) => ep.raw.as_raw_ref::<T>(),
    //         EndpointKind::Isochronous => {
    //             panic!("EndpointType::as_type_ref: Isochronous endpoint not implemented")
    //         }
    //         EndpointKind::Bulk => {
    //             panic!("EndpointType::as_type_ref: Bulk endpoint not implemented")
    //         }
    //         EndpointKind::Interrupt => {
    //             panic!("EndpointType::as_type_ref: Interrupt endpoint not implemented")
    //         }
    //     }
    // }
}

pub(crate) struct EndpointBase {
    raw: Box<dyn EndpointOp>,
}

impl EndpointBase {
    pub fn new(raw: impl EndpointOp) -> Self {
        Self { raw: Box::new(raw) }
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

    pub fn kernel(&self) -> &Kernel {
        self.raw.kernel()
    }

    pub fn submit(&mut self, transfer: Transfer) -> Result<TransferHandle<'_>, TransferError> {
        self.raw.submit(transfer)
    }

    pub(crate) fn as_raw_mut<T: EndpointOp>(&mut self) -> &mut T {
        let d = self.raw.as_mut() as &mut dyn Any;
        d.downcast_mut::<T>()
            .expect("EndpointBase downcast_mut failed")
    }

    // pub(crate) fn as_raw_ref<T: EndpointOp>(&self) -> &T {
    //     let d = self.raw.as_ref() as &dyn Any;
    //     d.downcast_ref::<T>()
    //         .expect("EndpointBase downcast_ref failed")
    // }
}

pub(crate) trait EndpointOp: Send + Any + 'static {
    fn submit(&mut self, transfer: Transfer) -> Result<TransferHandle<'_>, TransferError>;

    fn query_transfer(&mut self, id: u64) -> Option<Result<Transfer, TransferError>>;

    fn register_cx(&self, id: u64, cx: &mut Context<'_>);

    fn kernel(&self) -> &Kernel;
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
