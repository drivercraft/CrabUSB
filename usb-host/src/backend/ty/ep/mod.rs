use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::any::Any;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use usb_if::{
    err::TransferError,
    queue::{
        IsoPacketResult, QueueConfig, QueueEvent, RequestId, TransferCompletion, TransferRequest,
        TransferStatus,
    },
};

use super::transfer::Transfer;
use spin::Mutex;

mod ctrl;

pub use ctrl::*;

#[derive(Clone)]
pub(crate) struct EndpointBase {
    raw: Arc<Mutex<Box<dyn EndpointOp>>>,
}

impl EndpointBase {
    pub fn new(raw: impl EndpointOp) -> Self {
        Self {
            raw: Arc::new(Mutex::new(Box::new(raw))),
        }
    }

    pub fn queue(&self, config: QueueConfig) -> EndpointQueue {
        EndpointQueue {
            config,
            endpoint: self.raw.clone(),
        }
    }

    #[allow(unused)]
    pub(crate) fn with_raw_mut<T: EndpointOp, R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut raw = self.raw.lock();
        let d = raw.as_mut() as &mut dyn Any;
        f(d.downcast_mut::<T>()
            .expect("EndpointBase downcast_mut failed"))
    }
}

pub(crate) trait EndpointOp: Send + Any + 'static {
    fn submit_request(&mut self, request: TransferRequest) -> Result<RequestId, TransferError>;

    fn reclaim_request(
        &mut self,
        id: RequestId,
    ) -> Option<Result<TransferCompletion, TransferError>>;

    fn register_waker(&self, id: RequestId, cx: &mut Context<'_>);

    fn cancel_request(&mut self, _id: RequestId) -> Result<(), TransferError> {
        Err(TransferError::NotSupported)
    }

    fn handle_queue_event(&mut self, _event: QueueEvent) -> Result<(), TransferError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct EndpointQueue {
    config: QueueConfig,
    pub(crate) endpoint: Arc<Mutex<Box<dyn EndpointOp>>>,
}

impl EndpointQueue {
    pub(crate) fn new(config: QueueConfig, base: EndpointBase) -> Self {
        base.queue(config)
    }

    pub fn config(&self) -> QueueConfig {
        self.config
    }

    pub fn submit(&self, request: TransferRequest) -> Result<RequestId, TransferError> {
        self.endpoint.lock().submit_request(request)
    }

    pub fn reclaim(&self, id: RequestId) -> Result<Option<TransferCompletion>, TransferError> {
        match self.endpoint.lock().reclaim_request(id) {
            Some(result) => result.map(Some),
            None => Ok(None),
        }
    }

    pub fn poll_request(
        &self,
        id: RequestId,
        cx: &mut Context<'_>,
    ) -> Poll<Result<TransferCompletion, TransferError>> {
        let mut endpoint = self.endpoint.lock();
        match endpoint.reclaim_request(id) {
            Some(res) => Poll::Ready(res),
            None => {
                endpoint.register_waker(id, cx);
                Poll::Pending
            }
        }
    }

    pub fn cancel(&self, id: RequestId) -> Result<(), TransferError> {
        self.endpoint.lock().cancel_request(id)
    }

    pub fn handle_queue_event(&self, event: QueueEvent) -> Result<(), TransferError> {
        self.endpoint.lock().handle_queue_event(event)
    }

    pub async fn wait(
        &self,
        request: TransferRequest,
    ) -> Result<TransferCompletion, TransferError> {
        let id = self.submit(request)?;
        QueueRequestFuture {
            id,
            endpoint: self.endpoint.clone(),
        }
        .await
    }
}

struct QueueRequestFuture {
    id: RequestId,
    endpoint: Arc<Mutex<Box<dyn EndpointOp>>>,
}

impl Future for QueueRequestFuture {
    type Output = Result<TransferCompletion, TransferError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let id = self.id;
        let mut endpoint = self.endpoint.lock();
        match endpoint.reclaim_request(id) {
            Some(res) => Poll::Ready(res),
            None => {
                endpoint.register_waker(id, cx);
                Poll::Pending
            }
        }
    }
}

pub(crate) fn transfer_to_completion(id: RequestId, transfer: Transfer) -> TransferCompletion {
    let iso_packets = match &transfer.kind {
        usb_if::queue::TransferKind::Isochronous { packet_lengths } => packet_lengths
            .iter()
            .copied()
            .zip(transfer.iso_packet_actual_lengths.iter().copied())
            .map(|(requested_length, actual_length)| IsoPacketResult {
                requested_length,
                actual_length,
                status: TransferStatus::Completed,
            })
            .collect(),
        _ => Vec::new(),
    };

    TransferCompletion {
        request_id: id,
        status: TransferStatus::Completed,
        actual_length: transfer.transfer_len,
        iso_packets,
    }
}
