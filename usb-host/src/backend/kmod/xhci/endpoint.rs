use alloc::{collections::BTreeMap, sync::Arc, vec, vec::Vec};

use dma_api::DmaDirection;
use mbarrier::mb;
use spin::Mutex;
use usb_if::{
    descriptor::{self, EndpointDescriptor},
    err::TransferError,
    queue::{RequestId, TransferCompletion, TransferRequest},
    transfer::{BmRequestType, Direction},
};
use xhci::{
    registers::doorbell,
    ring::trb::{
        event::TransferEvent,
        transfer::{self, Isoch, Normal},
    },
};

use super::{DirectionExt, reg::SlotBell, ring::SendRing, transfer::TransferId};
use crate::{
    BusAddr,
    backend::{
        Dci,
        ty::{
            ep::{EndpointOp, transfer_to_completion},
            transfer::{Transfer, TransferKind},
        },
    },
    err::ConvertXhciError,
    osal::Kernel,
};

pub struct Endpoint {
    dci: Dci,
    pub ring: SendRing<TransferEvent>,
    bell: Arc<Mutex<SlotBell>>,
    transfers: BTreeMap<TransferId, Transfer>,
    iso_packet_ids: BTreeMap<TransferId, Vec<TransferId>>,
    trb_counts: BTreeMap<TransferId, usize>,
    outstanding_trbs: usize,
    kernel: Kernel,
    max_packet_size: usize,
    max_burst_size: usize,
}

unsafe impl Send for Endpoint {}
unsafe impl Sync for Endpoint {}

impl Endpoint {
    pub fn new(dci: Dci, kernel: &Kernel, bell: Arc<Mutex<SlotBell>>) -> crate::err::Result<Self> {
        let ring = SendRing::new(DmaDirection::Bidirectional, kernel)?;

        Ok(Self {
            dci,
            ring,
            bell,
            transfers: BTreeMap::new(),
            iso_packet_ids: BTreeMap::new(),
            trb_counts: BTreeMap::new(),
            outstanding_trbs: 0,
            kernel: kernel.clone(),
            max_packet_size: 0,
            max_burst_size: 0,
        })
    }

    pub fn configure_periodic(&mut self, max_packet_size: usize, max_burst_size: usize) {
        self.max_packet_size = max_packet_size;
        self.max_burst_size = max_burst_size;
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
        c: TransferEvent,
        handle: BusAddr,
    ) -> Result<Transfer, TransferError> {
        let handle = TransferId(handle);
        if let Some(count) = self.trb_counts.remove(&handle) {
            self.outstanding_trbs = self.outstanding_trbs.saturating_sub(count);
        }
        let mut t = self.transfers.remove(&handle).unwrap();
        match c.completion_code() {
            Ok(code) => match code.to_result() {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            },
            Err(_e) => Err(TransferError::Other(anyhow!("Transfer failed"))),
        }?;

        let transfer_len;
        if let TransferKind::Isochronous { packet_lengths } = &t.kind {
            let packet_ids = self
                .iso_packet_ids
                .remove(&handle)
                .unwrap_or_else(|| vec![handle]);
            if packet_ids.len() != packet_lengths.len() {
                return Err(TransferError::Other(anyhow!(
                    "ISO completion count mismatch: ids={}, packets={}",
                    packet_ids.len(),
                    packet_lengths.len()
                )));
            }

            let mut actual_lengths = Vec::with_capacity(packet_ids.len());
            for (index, packet_id) in packet_ids.iter().copied().enumerate() {
                let event = if packet_id == handle {
                    c
                } else {
                    self.ring.get_finished(packet_id.0).ok_or_else(|| {
                        TransferError::Other(anyhow!(
                            "missing ISO packet completion for {:?}",
                            packet_id
                        ))
                    })?
                };
                match event.completion_code() {
                    Ok(code) => code.to_result()?,
                    Err(_e) => return Err(TransferError::Other(anyhow!("Transfer failed"))),
                }

                let requested = packet_lengths[index];
                let remaining = event.trb_transfer_length() as usize;
                actual_lengths.push(requested.saturating_sub(remaining));
            }

            transfer_len = actual_lengths.iter().sum();
            t.iso_packet_actual_lengths = actual_lengths;
            if transfer_len > 0 && matches!(t.direction, Direction::In) {
                t.prepare_read_all();
            }
            t.transfer_len = transfer_len;
            trace!("ISO transfer data length: {}", t.transfer_len);
            return Ok(t);
        }

        let remaining = c.trb_transfer_length() as usize;
        transfer_len = t.buffer_len().saturating_sub(remaining);

        if transfer_len > 0 && matches!(t.direction, Direction::In) {
            // 刷新/失效缓存，确保从 DMA 缓冲读取到有效数据
            // t.dma_slice().prepare_read_all();
            t.prepare_read_all();
        }
        t.transfer_len = transfer_len;
        trace!("Transfer data length: {}", t.transfer_len);
        Ok(t)
    }

    fn enque_trb(&mut self, trb: transfer::Allowed) -> TransferId {
        TransferId(self.ring.enque_transfer(trb))
    }

    fn enque_iso(
        &mut self,
        bus_addr: u64,
        packet_lengths: &[usize],
        interrupt_on_short_packet: bool,
    ) -> (TransferId, Vec<TransferId>) {
        if packet_lengths.len() <= 1 {
            let id = self.enque_iso_trb(
                bus_addr,
                packet_lengths.first().copied().unwrap_or(0),
                false,
                true,
                interrupt_on_short_packet,
            );
            (id, vec![id])
        } else {
            self.enque_iso_multi(bus_addr, packet_lengths, interrupt_on_short_packet)
        }
    }

    fn enque_iso_trb(
        &mut self,
        bus_addr: u64,
        buff_len: usize,
        chain: bool,
        ioc: bool,
        interrupt_on_short_packet: bool,
    ) -> TransferId {
        let mut trb = Isoch::new();
        trb.set_data_buffer_pointer(bus_addr as _)
            .set_trb_transfer_length(buff_len as _)
            .set_interrupter_target(0)
            .set_start_isoch_asap();
        if interrupt_on_short_packet {
            trb.set_interrupt_on_short_packet();
        }
        let total_packets = if self.max_packet_size == 0 {
            1
        } else {
            buff_len.div_ceil(self.max_packet_size).max(1)
        };
        let packets_per_burst = self.max_burst_size.saturating_add(1).max(1);
        let burst_count = total_packets.div_ceil(packets_per_burst).saturating_sub(1);
        let last_burst_packet_count = match total_packets % packets_per_burst {
            0 => packets_per_burst.saturating_sub(1),
            residue => residue.saturating_sub(1),
        };
        trb.set_td_size_or_tbc(burst_count.min(0x1f) as u8)
            .set_transfer_last_burst_packet_count(last_burst_packet_count.min(0xf) as u8);
        if chain {
            trb.set_chain_bit();
        }
        if ioc {
            trb.set_interrupt_on_completion();
        }

        // 创建Isoch TRB
        let trb = transfer::Allowed::Isoch(trb);
        self.enque_trb(trb)
    }
    fn enque_iso_multi(
        &mut self,
        bus_addr: u64,
        packet_lengths: &[usize],
        interrupt_on_short_packet: bool,
    ) -> (TransferId, Vec<TransferId>) {
        let mut ids = Vec::with_capacity(packet_lengths.len());
        let mut offset = 0u64;

        for packet_length in packet_lengths.iter().copied() {
            let current_size = packet_length as u64;
            let current_addr = bus_addr + offset;

            ids.push(self.enque_iso_trb(
                current_addr,
                current_size as _,
                false,
                true,
                interrupt_on_short_packet,
            ));

            offset += current_size;
        }

        let id = ids.last().copied().unwrap_or(TransferId(BusAddr(0)));
        (id, ids)
    }

    fn required_trbs(transfer: &Transfer) -> usize {
        match &transfer.kind {
            TransferKind::Control(_) => {
                if transfer.buffer_len() > 0 {
                    3
                } else {
                    2
                }
            }
            TransferKind::Bulk | TransferKind::Interrupt => 1,
            TransferKind::Isochronous { packet_lengths } => packet_lengths.len().max(1),
        }
    }

    fn ensure_ring_capacity(&self, required: usize) -> Result<(), TransferError> {
        let usable = self.ring.usable_capacity().saturating_sub(1);
        if self.outstanding_trbs.saturating_add(required) > usable {
            return Err(TransferError::QueueFull);
        }
        Ok(())
    }
}

impl EndpointOp for Endpoint {
    fn submit_request(&mut self, request: TransferRequest) -> Result<RequestId, TransferError> {
        let transfer = Transfer::from_request(&self.kernel, request);
        let required_trbs = Self::required_trbs(&transfer);
        self.ensure_ring_capacity(required_trbs)?;

        let mut data_bus_addr = 0;
        if transfer.buffer_len() > 0 {
            // let data_slice = transfer.dma_slice();
            if matches!(transfer.direction, Direction::Out) {
                // data_slice.confirm_write_all();
                transfer.confirm_write_all();
            }
            // data_bus_addr = data_slice.bus_addr();
            data_bus_addr = transfer.dma_addr();

            // 检查缓冲区起始地址是否在 dma_mask 范围内
            assert!(
                data_bus_addr <= self.kernel.dma_mask(),
                "DMA address 0x{:x} exceeds controller DMA mask 0x{:x} ({}-bit addressing)",
                data_bus_addr,
                self.kernel.dma_mask(),
                if self.kernel.dma_mask() == u32::MAX as u64 {
                    32
                } else {
                    64
                }
            );

            // 检查缓冲区结束地址是否在 dma_mask 范围内
            let buffer_end = data_bus_addr + transfer.buffer_len() as u64;
            assert!(
                buffer_end <= self.kernel.dma_mask(),
                "DMA buffer end 0x{:x} (start: 0x{:x}, len: {} bytes) exceeds controller DMA mask 0x{:x} ({}-bit addressing)",
                buffer_end,
                data_bus_addr,
                transfer.buffer_len(),
                self.kernel.dma_mask(),
                if self.kernel.dma_mask() == u32::MAX as u64 {
                    32
                } else {
                    64
                }
            );
        }

        let data_len = transfer.buffer_len();
        let dir = transfer.direction;

        let mut handle = TransferId(BusAddr(0));
        let mut iso_packet_ids = Vec::new();

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

                if transfer.buffer_len() > 0 {
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

                if matches!(transfer.direction, Direction::In) && transfer.buffer_len() > 0 {
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
            TransferKind::Isochronous { packet_lengths } => {
                let ids = self.enque_iso(
                    data_bus_addr,
                    packet_lengths,
                    matches!(transfer.direction, Direction::In),
                );
                handle = ids.0;
                iso_packet_ids = ids.1;
            }
        }
        if !iso_packet_ids.is_empty() {
            self.iso_packet_ids.insert(handle, iso_packet_ids);
        }
        self.trb_counts.insert(handle, required_trbs);
        self.outstanding_trbs += required_trbs;
        self.transfers.insert(handle, transfer);
        mb();
        self.doorbell();

        Ok(RequestId::new(handle.0.raw()))
    }

    fn reclaim_request(
        &mut self,
        id: RequestId,
    ) -> Option<Result<TransferCompletion, TransferError>> {
        let raw_id = BusAddr(id.raw());
        let c = self.ring.get_finished(raw_id)?;
        let res = self
            .handle_transfer_completion(c, raw_id)
            .map(|transfer| transfer_to_completion(id, transfer));
        Some(res)
    }

    fn register_waker(&self, id: RequestId, cx: &mut core::task::Context<'_>) {
        self.ring.register_cx(BusAddr(id.raw()), cx);
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
