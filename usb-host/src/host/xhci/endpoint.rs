use dma_api::{DSlice, DSliceMut};
use log::trace;
use mbarrier::mb;
use xhci::{
    registers::doorbell,
    ring::trb::{
        event::CompletionCode,
        transfer::{self, Isoch, Normal},
    },
};

use crate::{
    BusAddr,
    endpoint::{direction, kind},
    err::USBError,
    standard::{
        self,
        descriptors::{EndpointDescriptor, EndpointType},
        transfer::Direction,
    },
    xhci::{def::Dci, device::DeviceState, ring::Ring},
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

    pub fn enque(
        &mut self,
        trbs: impl Iterator<Item = transfer::Allowed>,
        direction: Direction,
        buff_addr: usize,
        buff_len: usize,
    ) -> impl Future<Output = Result<usize, USBError>> {
        let mut trb_ptr = BusAddr(0);

        for trb in trbs {
            trb_ptr = self.ring.enque_transfer(trb);
        }

        trace!("trb : {trb_ptr:#x?}");

        mb();

        let mut bell = doorbell::Register::default();
        bell.set_doorbell_target(self.dci.into());

        self.device.doorbell(bell);

        let fur = unsafe { self.device.root.try_wait_for_transfer(trb_ptr).unwrap() };

        async move {
            let ret = fur.await;
            match ret.completion_code() {
                Ok(code) => {
                    if !matches!(code, CompletionCode::Success) {
                        return Err(USBError::TransferEventError(code));
                    }
                }
                Err(_e) => return Err(USBError::Unknown),
            }

            if buff_len > 0 {
                let data_slice =
                    unsafe { core::slice::from_raw_parts_mut(buff_addr as *mut u8, buff_len) };

                let dm = DSliceMut::from(
                    data_slice,
                    match direction {
                        Direction::Out => dma_api::Direction::ToDevice,
                        Direction::In => dma_api::Direction::FromDevice,
                    },
                );
                dm.preper_read_all();
            }
            Ok(ret.trb_transfer_length() as usize)
        }
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.ring.bus_addr()
    }
}

impl EndpointDescriptor {
    pub(crate) fn endpoint_type(&self) -> xhci::context::EndpointType {
        match self.transfer_type {
            standard::descriptors::EndpointType::Control => xhci::context::EndpointType::Control,
            standard::descriptors::EndpointType::Isochronous => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::IsochOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::IsochIn,
            },
            standard::descriptors::EndpointType::Bulk => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::BulkOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::BulkIn,
            },
            standard::descriptors::EndpointType::Interrupt => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::InterruptOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::InterruptIn,
            },
        }
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
        expected_direction: Direction,
        expected_type: EndpointType,
    ) -> Result<(), USBError> {
        if self.desc.direction != expected_direction {
            return Err(USBError::Unknown);
        }
        if self.desc.transfer_type != expected_type {
            return Err(USBError::Unknown);
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

    /// 创建Isoch TRB
    fn create_isoch_trb(&self, addr_bus: usize, len: usize) -> transfer::Allowed {
        transfer::Allowed::Isoch(
            *Isoch::new()
                .set_data_buffer_pointer(addr_bus as _)
                .set_trb_transfer_length(len as _)
                .set_interrupter_target(0)
                .set_interrupt_on_completion(),
        )
    }

    /// 执行传输的通用方法
    fn execute_transfer(
        &mut self,
        trb: transfer::Allowed,
        addr_virt: usize,
        len: usize,
    ) -> impl Future<Output = Result<usize, USBError>> {
        self.raw
            .enque([trb].into_iter(), self.desc.direction, addr_virt, len)
    }
}

impl Endpoint<kind::Bulk, direction::In> {
    pub fn transfer(
        &mut self,
        data: &mut [u8],
    ) -> Result<impl Future<Output = Result<usize, USBError>>, USBError> {
        self.validate_endpoint(Direction::In, EndpointType::Bulk)?;

        let (len, addr_virt, addr_bus) = self.prepare_in_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        Ok(self.execute_transfer(trb, addr_virt, len))
    }
}

impl Endpoint<kind::Bulk, direction::Out> {
    pub fn transfer(
        &mut self,
        data: &[u8],
    ) -> Result<impl Future<Output = Result<usize, USBError>>, USBError> {
        self.validate_endpoint(Direction::Out, EndpointType::Bulk)?;

        let (len, addr_virt, addr_bus) = self.prepare_out_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        Ok(self.execute_transfer(trb, addr_virt, len))
    }
}

impl Endpoint<kind::Interrupt, direction::In> {
    pub fn transfer(
        &mut self,
        data: &mut [u8],
    ) -> Result<impl Future<Output = Result<usize, USBError>>, USBError> {
        self.validate_endpoint(Direction::In, EndpointType::Interrupt)?;

        let (len, addr_virt, addr_bus) = self.prepare_in_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        Ok(self.execute_transfer(trb, addr_virt, len))
    }
}

impl Endpoint<kind::Interrupt, direction::Out> {
    pub fn transfer(
        &mut self,
        data: &[u8],
    ) -> Result<impl Future<Output = Result<usize, USBError>>, USBError> {
        self.validate_endpoint(Direction::Out, EndpointType::Interrupt)?;

        let (len, addr_virt, addr_bus) = self.prepare_out_buffer(data);
        let trb = self.create_normal_trb(addr_bus, len);

        Ok(self.execute_transfer(trb, addr_virt, len))
    }
}

impl Endpoint<kind::Isochronous, direction::In> {
    pub fn transfer(
        &mut self,
        data: &mut [u8],
    ) -> Result<impl Future<Output = Result<usize, USBError>>, USBError> {
        self.validate_endpoint(Direction::In, EndpointType::Isochronous)?;

        let (len, addr_virt, addr_bus) = self.prepare_in_buffer(data);
        let trb = self.create_isoch_trb(addr_bus, len);

        Ok(self.execute_transfer(trb, addr_virt, len))
    }
}

impl Endpoint<kind::Isochronous, direction::Out> {
    pub fn transfer(
        &mut self,
        data: &[u8],
    ) -> Result<impl Future<Output = Result<usize, USBError>>, USBError> {
        self.validate_endpoint(Direction::Out, EndpointType::Isochronous)?;

        let (len, addr_virt, addr_bus) = self.prepare_out_buffer(data);
        let trb = self.create_isoch_trb(addr_bus, len);

        Ok(self.execute_transfer(trb, addr_virt, len))
    }
}
