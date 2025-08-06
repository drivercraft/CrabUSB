use alloc::vec::Vec;
use dma_api::{DBox, DVec};
use xhci::context::{Device32Byte, Device64Byte, Input32Byte, Input64Byte, InputHandler};

use crate::{err::*, xhci::SlotId};

pub struct DeviceContextList {
    pub dcbaa: DVec<u64>,
    pub ctx_list: Vec<Option<ContextData>>,
    max_slots: usize,
}

struct Context32 {
    out: DBox<Device32Byte>,
    input: DBox<Input32Byte>,
}

struct Context64 {
    out: DBox<Device64Byte>,
    input: DBox<Input64Byte>,
}

pub struct ContextData {
    ctx64: Option<Context64>,
    ctx32: Option<Context32>,
}

impl ContextData {
    fn new(is_64: bool) -> Result<Self> {
        let ctx64;
        let ctx32;
        if is_64 {
            ctx64 = Some(Context64 {
                out: DBox::zero_with_align(dma_api::Direction::FromDevice, 64)
                    .ok_or(USBError::NoMemory)?,
                input: DBox::zero_with_align(dma_api::Direction::ToDevice, 64)
                    .ok_or(USBError::NoMemory)?,
            });
            ctx32 = None;
        } else {
            ctx32 = Some(Context32 {
                out: DBox::zero_with_align(dma_api::Direction::FromDevice, 64)
                    .ok_or(USBError::NoMemory)?,
                input: DBox::zero_with_align(dma_api::Direction::ToDevice, 64)
                    .ok_or(USBError::NoMemory)?,
            });
            ctx64 = None;
        }

        Ok(Self { ctx64, ctx32 })
    }

    pub fn dcbaa(&self) -> u64 {
        if let Some(ctx64) = &self.ctx64 {
            ctx64.out.bus_addr()
        } else if let Some(ctx32) = &self.ctx32 {
            ctx32.out.bus_addr()
        } else {
            panic!("No context available");
        }
    }

    pub fn input_perper_modify(&mut self) {
        self.with_input(|input| {
            let control_context = input.control_mut();
            for i in 0..32 {
                control_context.clear_add_context_flag(i);
                if i > 1 {
                    control_context.clear_drop_context_flag(i);
                }
            }
            control_context.set_add_context_flag(0);
        });
    }

    pub fn with_empty_input<F>(&mut self, f: F)
    where
        F: FnOnce(&mut dyn InputHandler),
    {
        if let Some(ctx64) = &mut self.ctx64 {
            let mut input = Input64Byte::new_64byte();
            f(&mut input);
            ctx64.input.write(input);
        } else if let Some(ctx32) = &mut self.ctx32 {
            let mut input = Input32Byte::new_32byte();
            f(&mut input);
            ctx32.input.write(input);
        } else {
            panic!("No context available");
        }
    }

    pub fn with_input<F>(&mut self, f: F)
    where
        F: FnOnce(&mut dyn InputHandler),
    {
        if let Some(ctx64) = &mut self.ctx64 {
            let mut input = ctx64.input.read();
            f(&mut input);
            ctx64.input.write(input);
        } else if let Some(ctx32) = &mut self.ctx32 {
            let mut input = ctx32.input.read();
            f(&mut input);
            ctx32.input.write(input);
        } else {
            panic!("No context available");
        }
    }

    pub fn input_bus_addr(&self) -> u64 {
        if let Some(ctx64) = &self.ctx64 {
            ctx64.input.bus_addr()
        } else if let Some(ctx32) = &self.ctx32 {
            ctx32.input.bus_addr()
        } else {
            panic!("No context available");
        }
    }
}

impl DeviceContextList {
    pub fn new(max_slots: usize) -> Result<Self> {
        let dcbaa =
            DVec::zeros(256, 0x1000, dma_api::Direction::ToDevice).ok_or(USBError::NoMemory)?;
        let mut ctx_list = Vec::with_capacity(max_slots);
        for _ in 0..max_slots {
            ctx_list.push(None);
        }

        Ok(Self {
            dcbaa,
            ctx_list,
            max_slots,
        })
    }

    pub fn new_ctx(&mut self, slot_id: SlotId, is_64: bool) -> Result<*mut ContextData> {
        if slot_id.as_usize() > self.max_slots {
            Err(USBError::SlotLimitReached)?;
        }
        let ctx = ContextData::new(is_64)?;
        self.dcbaa.set(slot_id.as_usize(), ctx.dcbaa());
        self.ctx_list[slot_id.as_usize()] = Some(ctx);
        let ctx_ptr = self.ctx_list[slot_id.as_usize()]
            .as_mut()
            .map(|c| c as *mut ContextData)
            .ok_or(USBError::NotFound)?;
        Ok(ctx_ptr)
    }
}

pub struct ScratchpadBufferArray {
    pub entries: DVec<u64>,
    pub _pages: Vec<DVec<u8>>,
}

impl ScratchpadBufferArray {
    pub fn new(entries: usize) -> Result<Self> {
        let mut entries_vec = DVec::zeros(entries, 64, dma_api::Direction::Bidirectional)
            .ok_or(USBError::NoMemory)?;

        let pages: Vec<DVec<u8>> = (0..entries_vec.len())
            .map(|_| {
                DVec::<u8>::zeros(0x1000, 0x1000, dma_api::Direction::Bidirectional)
                    .ok_or(USBError::NoMemory)
            })
            .try_collect()?;

        // 将每个页面的地址写入到 entries 数组中
        for (i, page) in pages.iter().enumerate() {
            entries_vec.set(i, page.bus_addr());
        }

        Ok(Self {
            entries: entries_vec,
            _pages: pages,
        })
    }

    pub fn bus_addr(&self) -> u64 {
        self.entries.bus_addr()
    }
}
