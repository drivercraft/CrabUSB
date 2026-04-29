use core::ptr::NonNull;

use alloc::{collections::BTreeMap, sync::Arc};

use dma_api::DmaDirection;
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

use super::{DirectionExt, reg::SlotBell, ring::SendRing, transfer::TransferId};
use crate::{
    BusAddr,
    backend::{
        Dci,
        ty::{
            ep::{EndpointOp, TransferHandle},
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
    kernel: Kernel,
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
            kernel: kernel.clone(),
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
            Err(_e) => Err(TransferError::Other(anyhow!("Transfer failed"))),
        }?;

        let transfer_len;

        // xHCI 规范：trb_transfer_length 字段根据端点方向有不同的含义
        // - IN 端点（设备到主机）：表示未传输的剩余字节数
        // - OUT 端点（主机到设备）：表示实际传输的字节数
        if matches!(t.direction, Direction::In) {
            // 对于 IN 端点，实际传输长度 = 请求长度 - 剩余长度
            transfer_len = t
                .buffer_len()
                .saturating_sub(c.trb_transfer_length() as usize);

            if transfer_len > 0 {
                // 刷新/失效缓存，确保从 DMA 缓冲读取到有效数据
                // t.dma_slice().prepare_read_all();
                t.prepare_read_all();
            }
        } else {
            // 对于 OUT 端点，trb_transfer_length 就是实际传输长度
            transfer_len = c.trb_transfer_length() as usize;
        }
        t.transfer_len = transfer_len;
        trace!("Transfer data length: {}", t.transfer_len);
        Ok(t)
    }

    fn enque_trb(&mut self, trb: transfer::Allowed) -> TransferId {
        TransferId(self.ring.enque_transfer(trb))
    }

    fn enque_iso(&mut self, bus_addr: u64, packet_lengths: &[usize]) -> TransferId {
        if packet_lengths.len() <= 1 {
            self.enque_iso_trb(
                bus_addr,
                packet_lengths.first().copied().unwrap_or(0),
                false,
                true,
            )
        } else {
            self.enque_iso_multi(bus_addr, packet_lengths)
        }
    }

    fn enque_iso_trb(
        &mut self,
        bus_addr: u64,
        buff_len: usize,
        chain: bool,
        ioc: bool,
    ) -> TransferId {
        let mut trb = Isoch::new();
        trb.set_data_buffer_pointer(bus_addr as _)
            .set_trb_transfer_length(buff_len as _)
            .set_interrupter_target(0)
            .set_start_isoch_asap();
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
    fn enque_iso_multi(&mut self, bus_addr: u64, packet_lengths: &[usize]) -> TransferId {
        let mut id = TransferId(BusAddr(0));
        let mut offset = 0u64;

        for (index, packet_length) in packet_lengths.iter().copied().enumerate() {
            let current_size = packet_length as u64;

            if current_size > 0 {
                let current_addr = bus_addr + offset;
                let is_last = index + 1 == packet_lengths.len();

                if index == 0 {
                    // 第一个TRB必须是Isoch TRB
                    id = self.enque_iso_trb(current_addr, current_size as _, !is_last, is_last);
                } else {
                    // 后续TRB使用Normal TRB
                    let mut trb = Normal::new();
                    trb.set_data_buffer_pointer(current_addr as _);
                    trb.set_trb_transfer_length(current_size as _);
                    trb.set_interrupter_target(0);

                    if is_last {
                        trb.set_interrupt_on_completion();
                    } else {
                        trb.set_chain_bit();
                    }
                    let trb = transfer::Allowed::Normal(trb);
                    id = self.enque_trb(trb);
                }
            }

            offset += current_size;
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
                handle = self.enque_iso(data_bus_addr, packet_lengths);
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

    fn new_transfer(
        &mut self,
        kind: TransferKind,
        direction: Direction,
        buff: Option<(NonNull<u8>, usize)>,
    ) -> Transfer {
        Transfer::new(&self.kernel, kind, direction, buff)
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
