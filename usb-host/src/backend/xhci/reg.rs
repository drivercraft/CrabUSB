use core::{
    num::NonZeroUsize,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use xhci::accessor::Mapper;

use crate::err::Result;

#[derive(Debug, Clone, Copy)]
pub struct MemMapper;
impl Mapper for MemMapper {
    unsafe fn map(&mut self, phys_start: usize, _bytes: usize) -> NonZeroUsize {
        unsafe { NonZeroUsize::new_unchecked(phys_start) }
    }
    fn unmap(&mut self, _virt_start: usize, _bytes: usize) {}
}

type Registers = xhci::Registers<MemMapper>;
// type RegistersExtList = xhci::extended_capabilities::List<MemMapper>;
// type SupportedProtocol = xhci::extended_capabilities::XhciSupportedProtocol<MemMapper>;

pub(crate) struct XhciRegisters {
    pub mmio_base: usize,
    reg: Registers,
}

impl Clone for XhciRegisters {
    fn clone(&self) -> Self {
        Self {
            mmio_base: self.mmio_base,
            reg: self.new_reg(),
        }
    }
}

impl XhciRegisters {
    pub fn new(mmio_base: NonNull<u8>) -> Self {
        let mmio_base = mmio_base.as_ptr() as usize;
        let mapper = MemMapper {};
        let reg = unsafe { Registers::new(mmio_base, mapper) };
        Self { mmio_base, reg }
    }

    fn new_reg(&self) -> Registers {
        let mapper = MemMapper {};
        unsafe { Registers::new(self.mmio_base, mapper) }
    }

    pub fn disable_irq_guard(&mut self) -> DisableIrqGuard {
        let mut enable = true;
        self.operational.usbcmd.update_volatile(|r| {
            enable = r.interrupter_enable();
            r.clear_interrupter_enable();
        });
        DisableIrqGuard {
            reg: self.new_reg(),
            enable,
        }
    }

    pub fn wait_for<'a>(
        &'a mut self,
        condition: impl Fn(&Registers) -> bool + 'a,
    ) -> impl core::future::Future<Output = ()> + 'a {
        WaitCondition {
            reg: &mut self.reg,
            condition,
        }
    }
}

struct WaitCondition<'a, F>
where
    F: Fn(&Registers) -> bool,
{
    reg: &'a mut Registers,
    condition: F,
}

impl<'a, F> Future for WaitCondition<'a, F>
where
    F: Fn(&Registers) -> bool,
{
    type Output = ();

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        if (self.condition)(self.reg) {
            core::task::Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}

impl Deref for XhciRegisters {
    type Target = Registers;

    fn deref(&self) -> &Self::Target {
        &self.reg
    }
}

impl DerefMut for XhciRegisters {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.reg
    }
}

pub struct DisableIrqGuard {
    reg: Registers,
    enable: bool,
}
impl Drop for DisableIrqGuard {
    fn drop(&mut self) {
        if self.enable {
            self.reg.operational.usbcmd.update_volatile(|r| {
                r.set_interrupter_enable();
            });
        }
    }
}
