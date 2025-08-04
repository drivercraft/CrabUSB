use core::ptr::NonNull;

use alloc::vec::Vec;

pub mod xhci;

use crate::err::*;
pub use xhci::Xhci;

define_int_type!(PortId, usize);

pub struct USBHost<C>
where
    C: Controller,
{
    ctrl: C,
}

impl<C> From<C> for USBHost<C>
where
    C: Controller,
{
    fn from(value: C) -> Self {
        Self { ctrl: value }
    }
}

impl USBHost<Xhci> {
    pub fn new(reg_base: NonNull<u8>) -> Self {
        Self::from(Xhci::new(reg_base))
    }

    pub async fn init(&mut self) -> Result {
        self.ctrl.init().await
    }

    pub async fn test_cmd(&mut self) -> Result {
        // for _ in 0..300 {
        self.ctrl.test_cmd().await?;
        // }

        Ok(())
    }

    pub async fn probe(&mut self) -> Result<Vec<xhci::Device>> {
        self.ctrl.probe().await
    }

    /// 中断处理
    ///
    /// # Safety
    ///
    /// 只能在中断函数中调用.
    pub unsafe fn handle_irq(&mut self) {
        self.ctrl.handle_irq();
    }
}

pub trait Controller: Send {
    type Device: IDevice;

    fn init(&mut self) -> impl Future<Output = Result> + Send;

    fn test_cmd(&mut self) -> impl Future<Output = Result> + Send;

    fn probe(&mut self) -> impl Future<Output = Result<Vec<Self::Device>>> + Send;

    fn handle_irq(&mut self) {}
}

pub trait IDevice: Send {}

pub mod endpoint {
    pub mod kind {
        pub struct Bulk;

        impl Sealed for Bulk {}

        pub struct Isochronous;

        impl Sealed for Isochronous {}
        pub struct Interrupt;

        impl Sealed for Interrupt {}

        pub trait Sealed {}
    }

    pub mod direction {
        pub struct In;

        impl Sealed for In {}
        pub struct Out;

        impl Sealed for Out {}
        pub trait Sealed {}
    }
}
