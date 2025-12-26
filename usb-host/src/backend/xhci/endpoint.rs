use alloc::sync::Arc;
use alloc::vec::Vec;

use dma_api::{DSlice, DSliceMut};
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
        xhci::{DirectionExt, reg::SlotBell, ring::SendRing},
    },
    err::ConvertXhciError,
};

pub(crate) struct EndpointRaw {
    dci: Dci,
    pub ring: SendRing<TransferEvent>,
    bell: Arc<Mutex<SlotBell>>,
}

unsafe impl Send for EndpointRaw {}
unsafe impl Sync for EndpointRaw {}

impl EndpointRaw {
    pub fn new(dci: Dci, dma_mask: usize, bell: Arc<Mutex<SlotBell>>) -> crate::err::Result<Self> {
        let ring = SendRing::new(dma_api::Direction::Bidirectional, dma_mask)?;

        Ok(Self { dci, ring, bell })
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

    pub async fn request(
        &mut self,
        trbs: impl Iterator<Item = transfer::Allowed>,
        direction: usb_if::transfer::Direction,
        buff_addr: usize,
        buff_len: usize,
    ) -> Result<usize, TransferError> {
        let mut trb_ptr = BusAddr(0);

        for trb in trbs {
            trb_ptr = self.ring.enque_transfer(trb);
        }

        trace!("trb : {trb_ptr:#x?}, addr: {buff_addr:#x}, len: {buff_len}, {direction:?}");

        mb();

        self.doorbell();
        trace!("ring doorbell done");

        let c = self.ring.wait_command_finished(trb_ptr).await;
        match c.completion_code() {
            Ok(code) => match code.to_result() {
                Ok(_) => {}
                Err(e) => Err(e)?,
            },
            Err(_e) => Err(TransferError::Other("Transfer failed".into()))?,
        };

        Ok(c.trb_transfer_length() as usize)
    }
}

pub struct EndpintControl {
    raw: EndpointRaw,
}

impl EndpintControl {
    pub fn new(raw: EndpointRaw) -> Self {
        Self { raw }
    }

    async fn transfer(
        &mut self,
        urb: ControlSetup,
        dir: Direction,
        buff: Option<(usize, u16)>,
    ) -> Result<usize, TransferError> {
        let mut trbs: Vec<transfer::Allowed> = Vec::new();
        let bm_request_type = BmRequestType {
            direction: dir,
            request_type: urb.request_type,
            recipient: urb.recipient,
        };

        let mut setup = transfer::SetupStage::default();
        let mut buff_data = 0;
        let mut buff_len = 0;

        setup
            .set_request_type(bm_request_type.into())
            .set_request(urb.request.into())
            .set_value(urb.value)
            .set_index(urb.index)
            .set_length(0)
            .set_transfer_type(transfer::TransferType::No);

        let mut data = None;

        if let Some((addr, len)) = buff {
            buff_data = addr;
            buff_len = len as usize;
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) };

            let dm = DSliceMut::from(data_slice, dma_api::Direction::Bidirectional);

            if matches!(dir, Direction::Out) {
                dm.confirm_write_all();
            }

            setup
                .set_transfer_type(dir.to_xhci_transfer_type())
                .set_length(len);

            let mut raw_data = transfer::DataStage::default();
            raw_data
                .set_data_buffer_pointer(dm.bus_addr() as _)
                .set_trb_transfer_length(len as _)
                .set_direction(dir.to_xhci_direction());

            data = Some(raw_data)
        }

        let mut status = transfer::StatusStage::default();
        status.set_interrupt_on_completion();

        if matches!(dir, Direction::In) && buff.is_some() {
            status.clear_direction();
        } else {
            status.set_direction();
        }

        trbs.push(setup.into());
        if let Some(data) = data {
            trbs.push(data.into());
        }
        trbs.push(status.into());

        self.raw
            .request(trbs.into_iter(), dir, buff_data, buff_len)
            .await
    }

    pub async fn control_in(
        &mut self,
        param: ControlSetup,
        buff: &mut [u8],
    ) -> Result<usize, TransferError> {
        let n = self
            .transfer(
                param,
                Direction::In,
                if buff.is_empty() {
                    None
                } else {
                    Some((buff.as_ptr() as usize, buff.len() as _))
                },
            )
            .await?;
        let dm = DSlice::from(&buff[..n], dma_api::Direction::Bidirectional);
        dm.prepare_read_all();
        Ok(n)
    }

    pub async fn control_out(
        &mut self,
        param: ControlSetup,
        buff: &[u8],
    ) -> Result<usize, TransferError> {
        self.transfer(
            param,
            Direction::Out,
            if buff.is_empty() {
                None
            } else {
                Some((buff.as_ptr() as usize, buff.len() as _))
            },
        )
        .await
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.raw.bus_addr()
    }
}
