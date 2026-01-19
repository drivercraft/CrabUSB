use alloc::{collections::BTreeMap, sync::Arc};

use mbarrier::mb;
use spin::Mutex;
use usb_if::{
    descriptor::{self, EndpointDescriptor},
    err::TransferError,
    transfer::{BmRequestType, Direction},
};
use xhci::{
    registers::doorbell,
    ring::trb::{
        event::TransferEvent,
        transfer::{self, Isoch, Normal},
    },
};

use crate::{
    BusAddr,
    backend::{
        Dci,
        ty::{
            ep::{EndpointOp, TransferHandle},
            transfer::{Transfer, TransferKind},
        },
        xhci::{DirectionExt, reg::SlotBell, ring::SendRing, transfer::TransferId},
    },
    err::ConvertXhciError,
};

pub struct Endpoint {
    dci: Dci,
    pub ring: SendRing<TransferEvent>,
    bell: Arc<Mutex<SlotBell>>,
    transfers: BTreeMap<TransferId, Transfer>,
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

    fn handle_transfer_completion(
        &mut self,
        c: &TransferEvent,
        handle: BusAddr,
    ) -> Result<Transfer, TransferError> {
        let mut t = self.transfers.remove(&TransferId(handle)).unwrap();
        match c.completion_code() {
            Ok(code) => match code.to_result() {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            },
            Err(_e) => Err(TransferError::Other("Transfer failed".into())),
        }?;
        t.transfer_len = c.trb_transfer_length() as usize;
        if matches!(t.direction, Direction::In) && t.buffer_len > 0 {
            trace!(
                "Preparing read all for transfer at addr {:#x}, len {}",
                t.buffer_addr, t.transfer_len
            );
            t.dma_slice().prepare_read_all();
        }
        Ok(t)
    }

    fn enque_trb(&mut self, trb: transfer::Allowed) -> TransferId {
        TransferId(self.ring.enque_transfer(trb))
    }

    fn enque_iso(&mut self, bus_addr: u64, buff_len: usize, num_iso_packets: usize) -> TransferId {
        if buff_len == 0 || num_iso_packets < 2 {
            self.enque_iso_trb(bus_addr, buff_len)
        } else {
            self.enque_iso_multi(bus_addr, buff_len, num_iso_packets)
        }
    }

    fn enque_iso_trb(&mut self, bus_addr: u64, buff_len: usize) -> TransferId {
        let mut trb = Isoch::new();
        trb.set_data_buffer_pointer(bus_addr as _)
            .set_trb_transfer_length(buff_len as _)
            .set_interrupter_target(0)
            .set_interrupt_on_completion();

        // if use_sia {
        //     trb.set_start_isoch_asap(); // 启用SIA
        // }

        // 创建Isoch TRB
        let trb = transfer::Allowed::Isoch(trb);
        self.enque_trb(trb)
    }
    fn enque_iso_multi(&mut self, bus_addr: u64, len: usize, num_iso_packets: usize) -> TransferId {
        let len = len as u64;
        let packet_size = if len == 0 {
            0
        } else {
            len.div_ceil(num_iso_packets as u64)
        };

        let mut id = TransferId(BusAddr(0));

        for i in 0..num_iso_packets {
            let i = i as u64;
            let offset = i * packet_size;
            if offset >= len {
                break; // 避免越界
            }
            let remaining = len - offset;
            let current_size = if remaining >= packet_size {
                packet_size
            } else {
                remaining
            };

            if current_size > 0 {
                let current_addr = bus_addr + offset;
                let is_last = (i == num_iso_packets as u64 - 1) || (offset + current_size >= len);

                if i == 0 {
                    // 第一个TRB必须是Isoch TRB
                    id = self.enque_iso_trb(current_addr, current_size as _);
                } else {
                    // 后续TRB使用Normal TRB
                    let mut trb = Normal::new();
                    trb.set_data_buffer_pointer(current_addr as _);
                    trb.set_trb_transfer_length(current_size as _);
                    trb.set_interrupter_target(0);

                    if is_last {
                        trb.set_interrupt_on_completion();
                    }
                    let trb = transfer::Allowed::Normal(trb);
                    id = self.enque_trb(trb);
                }
            }
        }

        id
    }
}

impl EndpointOp for Endpoint {
    fn submit(
        &mut self,
        transfer: crate::backend::ty::transfer::Transfer,
    ) -> Result<crate::backend::ty::ep::TransferHandle<'_>, TransferError> {
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

        let mut handle = TransferId(BusAddr(0));

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
            TransferKind::Interrupt | TransferKind::Bulk => {
                let trb = transfer::Allowed::Normal(
                    *Normal::new()
                        .set_data_buffer_pointer(data_bus_addr as _)
                        .set_trb_transfer_length(data_len as _)
                        .set_interrupter_target(0)
                        .set_interrupt_on_short_packet()
                        .set_interrupt_on_completion(),
                );
                handle.0 = self.ring.enque_transfer(trb);
            }
            TransferKind::Isochronous { num_pkgs } => {
                handle = self.enque_iso(data_bus_addr, data_len, *num_pkgs);
            }
        }
        self.transfers.insert(handle, transfer);
        mb();
        self.doorbell();

        Ok(TransferHandle::new(handle.0.raw(), self))
    }

    fn query_transfer(
        &mut self,
        id: u64,
    ) -> Option<Result<crate::backend::ty::transfer::Transfer, TransferError>> {
        let id = BusAddr(id);
        let c = self.ring.get_finished(id)?;
        let res = self.handle_transfer_completion(&c, id);
        Some(res)
    }

    fn register_cx(&self, id: u64, cx: &mut core::task::Context<'_>) {
        self.ring.register_cx(BusAddr(id), cx);
    }
}

pub(crate) trait EndpointDescriptorExt {
    fn endpoint_type(&self) -> xhci::context::EndpointType;
}

impl EndpointDescriptorExt for EndpointDescriptor {
    fn endpoint_type(&self) -> xhci::context::EndpointType {
        match self.transfer_type {
            descriptor::EndpointType::Control => xhci::context::EndpointType::Control,
            descriptor::EndpointType::Isochronous => match self.direction {
                usb_if::transfer::Direction::Out => xhci::context::EndpointType::IsochOut,
                usb_if::transfer::Direction::In => xhci::context::EndpointType::IsochIn,
            },
            descriptor::EndpointType::Bulk => match self.direction {
                usb_if::transfer::Direction::Out => xhci::context::EndpointType::BulkOut,
                usb_if::transfer::Direction::In => xhci::context::EndpointType::BulkIn,
            },
            descriptor::EndpointType::Interrupt => match self.direction {
                usb_if::transfer::Direction::Out => xhci::context::EndpointType::InterruptOut,
                usb_if::transfer::Direction::In => xhci::context::EndpointType::InterruptIn,
            },
        }
    }
}
