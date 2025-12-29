use core::pin::Pin;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use mbarrier::mb;
use spin::Mutex;
use usb_if::{
    err::TransferError,
    host::ControlSetup,
    transfer::{BmRequestType, Direction},
};
use xhci::{
    registers::doorbell,
    ring::trb::{event::TransferEvent, transfer},
};

use crate::{
    BusAddr,
    backend::{
        Dci,
        ty::EndpintOp,
        xhci::{
            DirectionExt,
            reg::SlotBell,
            ring::SendRing,
            transfer::{Transfer, TransferHandle, TransferKind},
        },
    },
    err::ConvertXhciError,
};

pub struct Endpoint {
    dci: Dci,
    pub ring: SendRing<TransferEvent>,
    bell: Arc<Mutex<SlotBell>>,
    transfers: BTreeMap<TransferHandle, Transfer>,
}

unsafe impl Send for Endpoint {}
unsafe impl Sync for Endpoint {}

impl Endpoint {
    pub fn new(dci: Dci, dma_mask: usize, bell: Arc<Mutex<SlotBell>>) -> crate::err::Result<Self> {
        let ring = SendRing::new(dma_api::Direction::Bidirectional, dma_mask)?;

        Ok(Self {
            dci,
            ring,
            bell,
            transfers: BTreeMap::new(),
        })
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.ring.bus_addr()
    }

    fn doorbell(&mut self) {
        let mut bell = doorbell::Register::default();
        bell.set_doorbell_target(self.dci.into());
        self.bell.lock().ring(bell);
    }

    pub fn ring(&self) -> &SendRing<TransferEvent> {
        &self.ring
    }

    pub fn submit(&mut self, transfer: Transfer) -> TransferHandle {
        let mut data_bus_addr = 0;
        if transfer.buffer_len > 0 {
            let data_slice = transfer.dma_slice();
            if matches!(transfer.direction, Direction::Out) {
                data_slice.confirm_write_all();
            }
            data_bus_addr = data_slice.bus_addr();
        }

        let data_len = transfer.buffer_len;
        let dir = transfer.direction;

        let mut handle = TransferHandle(BusAddr(0));

        match &transfer.kind {
            TransferKind::Control(t) => {
                let bm_request_type = BmRequestType {
                    direction: transfer.direction,
                    request_type: t.request_type,
                    recipient: t.recipient,
                };

                let mut setup = transfer::SetupStage::default();
                setup
                    .set_request_type(bm_request_type.into())
                    .set_request(t.request.into())
                    .set_value(t.value)
                    .set_index(t.index)
                    .set_length(0)
                    .set_transfer_type(transfer::TransferType::No);

                let mut data = None;

                if transfer.buffer_len > 0 {
                    setup
                        .set_transfer_type(dir.to_xhci_transfer_type())
                        .set_length(data_len as _);

                    let mut _data = transfer::DataStage::default();
                    _data
                        .set_data_buffer_pointer(data_bus_addr)
                        .set_trb_transfer_length(data_len as _)
                        .set_direction(transfer.direction.to_xhci_direction());
                    data = Some(_data);
                }

                let mut status = transfer::StatusStage::default();
                status.set_interrupt_on_completion();

                if matches!(transfer.direction, Direction::In) && transfer.buffer_len > 0 {
                    status.clear_direction();
                } else {
                    status.set_direction();
                }

                self.ring.enque_transfer(setup.into());
                if let Some(data) = data {
                    self.ring.enque_transfer(data.into());
                }
                handle.0 = self.ring.enque_transfer(status.into());
            }
        }
        self.transfers.insert(handle, transfer);
        mb();
        self.doorbell();

        handle
    }

    pub fn query(&mut self, handle: &TransferHandle) -> Option<Result<Transfer, TransferError>> {
        let c = self.ring.get_finished(handle.0)?;
        Some(self.handle_transfer_completion(&c, handle))
    }

    async fn async_query(&mut self, handle: TransferHandle) -> Result<Transfer, TransferError> {
        let c = self.ring.wait_command_finished(handle.0).await;
        self.handle_transfer_completion(&c, &handle)
    }

    fn handle_transfer_completion(
        &mut self,
        c: &TransferEvent,
        handle: &TransferHandle,
    ) -> Result<Transfer, TransferError> {
        let mut t = self.transfers.remove(handle).unwrap();
        match c.completion_code() {
            Ok(code) => match code.to_result() {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            },
            Err(_e) => Err(TransferError::Other("Transfer failed".into())),
        }?;
        if matches!(t.direction, Direction::In) && t.buffer_len > 0 {
            t.dma_slice().prepare_read_all();
        }
        t.transfer_len = c.trb_transfer_length() as usize;
        Ok(t)
    }

    fn request(
        &mut self,
        transfer: Transfer,
    ) -> impl Future<Output = Result<Transfer, TransferError>> {
        let handle = self.submit(transfer);
        self.async_query(handle)
    }
}

pub struct EndpintControl {
    raw: Endpoint,
}

impl EndpintControl {
    pub fn new(raw: Endpoint) -> Self {
        Self { raw }
    }

    pub async fn control_in(
        &mut self,
        param: ControlSetup,
        buff: &mut [u8],
    ) -> Result<usize, TransferError> {
        let transfer = Transfer::new_in(TransferKind::Control(param), Pin::new(buff));
        let t = self.raw.request(transfer).await?;
        let n = t.transfer_len;

        Ok(n)
    }

    pub async fn control_out(
        &mut self,
        param: ControlSetup,
        buff: &[u8],
    ) -> Result<usize, TransferError> {
        let transfer = Transfer::new_out(TransferKind::Control(param), Pin::new(buff));
        let t = self.raw.request(transfer).await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.raw.bus_addr()
    }
}

impl EndpintOp for Endpoint {
    type Transfer = Transfer;
}
