use alloc::{sync::Arc, vec::Vec};

use dma_api::{DSlice, DSliceMut};
use log::trace;
use mbarrier::mb;
use spin::Mutex;
use usb_if::{
    descriptor::{self, EndpointDescriptor, EndpointType},
    err::TransferError,
    host::{ControlSetup, ResultTransfer},
    transfer::{BmRequestType, Direction, wait::WaitOnReady},
};
use xhci::{
    registers::doorbell,
    ring::trb::transfer::{self, Isoch, Normal},
};

use crate::{
    BusAddr,
    endpoint::{direction, kind},
    err::USBError,
    host::xhci::{
        def::{Dci, DirectionExt},
        device::DeviceState,
        ring::Ring,
        root::Root,
    },
};

pub(crate) struct EndpointRaw {
    dci: Dci,
    pub ring: Ring,
    device: DeviceState,
}

unsafe impl Send for EndpointRaw {}

impl EndpointRaw {
    pub fn new(dci: Dci, device: &DeviceState) -> Result<Self, USBError> {
        Ok(Self {
            dci,
            ring: Ring::new(true, dma_api::Direction::Bidirectional)?,
            device: device.clone(),
        })
    }

    pub fn enque<'a>(
        &mut self,
        trbs: impl Iterator<Item = transfer::Allowed>,
        direction: usb_if::transfer::Direction,
        buff_addr: usize,
        buff_len: usize,
    ) -> ResultTransfer<'a> {
        let mut trb_ptr = BusAddr(0);

        for trb in trbs {
            trb_ptr = self.ring.enque_transfer(trb);
        }

        trace!("trb : {trb_ptr:#x?}");

        mb();

        let mut bell = doorbell::Register::default();
        bell.set_doorbell_target(self.dci.into());

        self.device.doorbell(bell);

        Ok(unsafe {
            self.device
                .root
                .try_wait_for_transfer(
                    trb_ptr,
                    WaitOnReady {
                        on_ready,
                        addr: buff_addr,
                        len: buff_len,
                        direction,
                    },
                )
                .unwrap()
        })
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.ring.bus_addr()
    }
}

fn on_ready(addr: usize, len: usize, direction: usb_if::transfer::Direction) {
    trace!("Transfer completed: addr={addr:#x}, len={len}");
    if len > 0 {
        let data_slice = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len) };

        let dm = DSliceMut::from(
            data_slice,
            match direction {
                usb_if::transfer::Direction::Out => dma_api::Direction::ToDevice,
                usb_if::transfer::Direction::In => dma_api::Direction::FromDevice,
            },
        );
        dm.preper_read_all();
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

#[derive(Clone)]
pub(crate) struct EndpointControl(Arc<Mutex<EndpointRaw>>);

impl EndpointControl {
    pub fn new(raw: EndpointRaw) -> Self {
        Self(Arc::new(Mutex::new(raw)))
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.0.lock().bus_addr()
    }

    pub fn transfer<'a>(
        &self,
        urb: ControlSetup,
        dir: Direction,
        buff: Option<(usize, u16)>,
    ) -> ResultTransfer<'a> {
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

        if matches!(dir, Direction::In) {
            status.set_direction();
        }

        trbs.push(setup.into());
        if let Some(data) = data {
            trbs.push(data.into());
        }
        trbs.push(status.into());

        self.0
            .lock()
            .enque(trbs.into_iter(), dir, buff_data, buff_len)
    }

    pub fn listen(&self, root: &mut Root) {
        let ring = self.0.lock();
        root.litsen_transfer(&ring.ring);
    }

    pub fn control_in<'a>(
        &mut self,
        param: ControlSetup,
        buff: &'a mut [u8],
    ) -> ResultTransfer<'a> {
        self.transfer(
            param,
            Direction::In,
            if buff.is_empty() {
                None
            } else {
                Some((buff.as_ptr() as usize, buff.len() as _))
            },
        )
    }

    pub fn control_out<'a>(&mut self, param: ControlSetup, buff: &'a [u8]) -> ResultTransfer<'a> {
        self.transfer(
            param,
            Direction::Out,
            if buff.is_empty() {
                None
            } else {
                Some((buff.as_ptr() as usize, buff.len() as _))
            },
        )
    }
}

pub struct Endpoint<T: kind::Sealed, D: direction::Sealed> {
    pub(crate) raw: EndpointRaw,
    desc: EndpointDescriptor,
    _marker: core::marker::PhantomData<(T, D)>,
}

impl<T: kind::Sealed, D: direction::Sealed> Endpoint<T, D> {
    pub(crate) fn new(desc: EndpointDescriptor, raw: EndpointRaw) -> Result<Self, USBError> {
        Ok(Self {
            raw,
            desc,
            _marker: core::marker::PhantomData,
        })
    }

    /// 验证端点方向和传输类型
    fn validate_endpoint(
        &self,
        expected_direction: usb_if::transfer::Direction,
        expected_type: usb_if::descriptor::EndpointType,
    ) -> Result<(), TransferError> {
        if self.desc.direction != expected_direction {
            return Err(TransferError::Other("Endpoint direction mismatch".into()));
        }
        if self.desc.transfer_type != expected_type {
            return Err(TransferError::Other("Endpoint type mismatch".into()));
        }
        Ok(())
    }

    /// 准备DMA缓冲区（输入方向）
    fn prepare_in_buffer(&self, data: &mut [u8]) -> (usize, usize, usize) {
        let len = data.len();
        let addr_virt = data.as_mut_ptr() as usize;
        let mut addr_bus = 0;

        if len > 0 {
            let dm = DSliceMut::from(data, dma_api::Direction::FromDevice);
            addr_bus = dm.bus_addr() as usize;
        }

        (len, addr_virt, addr_bus)
    }

    /// 准备DMA缓冲区（输出方向）
    fn prepare_out_buffer(&self, data: &[u8]) -> (usize, usize, usize) {
        let len = data.len();
        let addr_virt = data.as_ptr() as usize;
        let mut addr_bus = 0;

        if len > 0 {
            let dm = DSlice::from(data, dma_api::Direction::ToDevice);
            dm.confirm_write_all();
            addr_bus = dm.bus_addr() as usize;
        }

        (len, addr_virt, addr_bus)
    }

    /// 创建Normal TRB
    fn create_normal_trb(&self, addr_bus: usize, len: usize) -> transfer::Allowed {
        transfer::Allowed::Normal(
            *Normal::new()
                .set_data_buffer_pointer(addr_bus as _)
                .set_trb_transfer_length(len as _)
                .set_interrupter_target(0)
                .set_interrupt_on_short_packet()
                .set_interrupt_on_completion(),
        )
    }

    /// 创建Isoch TRB，根据xHCI规范优化
    fn create_isoch_trb(&self, addr_bus: usize, len: usize) -> transfer::Allowed {
        transfer::Allowed::Isoch(
            *Isoch::new()
                .set_data_buffer_pointer(addr_bus as _)
                .set_trb_transfer_length(len as _)
                .set_interrupter_target(0)
                .set_interrupt_on_completion(), // .set_start_isoch_asap() // 如果方法可用则启用
        )
    }

    /// 创建Normal TRB用于Isoch TD的链接
    fn create_normal_trb_for_isoch(
        &self,
        addr_bus: usize,
        len: usize,
        is_last: bool,
    ) -> transfer::Allowed {
        if is_last {
            transfer::Allowed::Normal(
                *Normal::new()
                    .set_data_buffer_pointer(addr_bus as _)
                    .set_trb_transfer_length(len as _)
                    .set_interrupter_target(0)
                    .set_interrupt_on_completion(),
            )
        } else {
            transfer::Allowed::Normal(
                *Normal::new()
                    .set_data_buffer_pointer(addr_bus as _)
                    .set_trb_transfer_length(len as _)
                    .set_interrupter_target(0),
            )
        }
    }

    /// 执行传输的通用方法
    fn execute_transfer<'a>(
        &mut self,
        trb: transfer::Allowed,
        addr_virt: usize,
        len: usize,
    ) -> ResultTransfer<'a> {
        self.raw
            .enque([trb].into_iter(), self.desc.direction, addr_virt, len)
    }

    /// 执行多包ISO传输的通用方法
    fn execute_iso_transfer<'a>(
        &mut self,
        trbs: Vec<transfer::Allowed>,
        addr_virt: usize,
        len: usize,
    ) -> ResultTransfer<'a> {
        self.raw
            .enque(trbs.into_iter(), self.desc.direction, addr_virt, len)
    }

    /// 创建多包ISO传输的TRB列表
    fn create_iso_trb_list(
        &self,
        addr_bus: usize,
        len: usize,
        num_iso_packets: usize,
    ) -> Vec<transfer::Allowed> {
        let mut trbs = Vec::new();
        let packet_size = if len == 0 {
            0
        } else {
            len.div_ceil(num_iso_packets)
        };

        for i in 0..num_iso_packets {
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
                let current_addr = addr_bus + offset;
                let is_last = (i == num_iso_packets - 1) || (offset + current_size >= len);

                if i == 0 {
                    // 第一个TRB必须是Isoch TRB
                    let trb = self.create_isoch_trb(current_addr, current_size);
                    trbs.push(trb);
                } else {
                    // 后续TRB使用Normal TRB
                    let trb = self.create_normal_trb_for_isoch(current_addr, current_size, is_last);
                    trbs.push(trb);
                }
            }
        }

        trbs
    }
}

impl Endpoint<kind::Bulk, direction::In> {
    pub fn transfer<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.validate_endpoint(Direction::In, usb_if::descriptor::EndpointType::Bulk)?;

        let (len, addr_virt, addr_bus) = self.prepare_in_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        self.execute_transfer(trb, addr_virt, len)
    }
}

impl Endpoint<kind::Bulk, direction::Out> {
    pub fn transfer<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.validate_endpoint(Direction::Out, usb_if::descriptor::EndpointType::Bulk)?;

        let (len, addr_virt, addr_bus) = self.prepare_out_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        self.execute_transfer(trb, addr_virt, len)
    }
}

impl Endpoint<kind::Interrupt, direction::In> {
    pub fn transfer<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.validate_endpoint(Direction::In, usb_if::descriptor::EndpointType::Interrupt)?;

        let (len, addr_virt, addr_bus) = self.prepare_in_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        self.execute_transfer(trb, addr_virt, len)
    }
}

impl Endpoint<kind::Interrupt, direction::Out> {
    pub fn transfer<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.validate_endpoint(Direction::Out, EndpointType::Interrupt)?;

        let (len, addr_virt, addr_bus) = self.prepare_out_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        self.execute_transfer(trb, addr_virt, len)
    }
}

impl Endpoint<kind::Isochronous, direction::In> {
    pub fn transfer_multi_packet<'a>(
        &mut self,
        data: &'a mut [u8],
        num_iso_packets: usize,
    ) -> ResultTransfer<'a> {
        self.validate_endpoint(Direction::In, EndpointType::Isochronous)?;

        let (len, addr_virt, addr_bus) = self.prepare_in_buffer(data);

        if num_iso_packets <= 1 || len == 0 {
            // 单包传输，使用原有逻辑
            let trb = self.create_isoch_trb(addr_bus, len);
            return self.execute_transfer(trb, addr_virt, len);
        }

        // 多包传输，使用公共方法创建TRB列表
        let trbs = self.create_iso_trb_list(addr_bus, len, num_iso_packets);
        self.execute_iso_transfer(trbs, addr_virt, len)
    }
}

impl Endpoint<kind::Isochronous, direction::Out> {
    pub fn transfer_multi_packet<'a>(
        &mut self,
        data: &'a [u8],
        num_iso_packets: usize,
    ) -> ResultTransfer<'a> {
        self.validate_endpoint(Direction::Out, EndpointType::Isochronous)?;

        let (len, addr_virt, addr_bus) = self.prepare_out_buffer(data);

        if num_iso_packets <= 1 || len == 0 {
            // 单包传输，使用原有逻辑
            let trb = self.create_isoch_trb(addr_bus, len);
            return self.execute_transfer(trb, addr_virt, len);
        }

        // 多包传输，使用公共方法创建TRB列表
        let trbs = self.create_iso_trb_list(addr_bus, len, num_iso_packets);
        self.execute_iso_transfer(trbs, addr_virt, len)
    }
}

impl<T, D> usb_if::host::TEndpint for Endpoint<T, D>
where
    T: kind::Sealed,
    D: direction::Sealed,
{
}

impl usb_if::host::EndpointBulkIn for Endpoint<kind::Bulk, direction::In> {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.transfer(data)
    }
}

impl usb_if::host::EndpointBulkOut for Endpoint<kind::Bulk, direction::Out> {
    fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.transfer(data)
    }
}

impl usb_if::host::EndpointInterruptIn for Endpoint<kind::Interrupt, direction::In> {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> ResultTransfer<'a> {
        self.transfer(data)
    }
}

impl usb_if::host::EndpointInterruptOut for Endpoint<kind::Interrupt, direction::Out> {
    fn submit<'a>(&mut self, data: &'a [u8]) -> ResultTransfer<'a> {
        self.transfer(data)
    }
}

impl usb_if::host::EndpintIsoIn for Endpoint<kind::Isochronous, direction::In> {
    fn submit<'a>(&mut self, data: &'a mut [u8], num_iso_packets: usize) -> ResultTransfer<'a> {
        self.transfer_multi_packet(data, num_iso_packets)
    }
}

impl usb_if::host::EndpintIsoOut for Endpoint<kind::Isochronous, direction::Out> {
    fn submit<'a>(&mut self, data: &'a [u8], num_iso_packets: usize) -> ResultTransfer<'a> {
        self.transfer_multi_packet(data, num_iso_packets)
    }
}
