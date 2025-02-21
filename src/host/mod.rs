use core::ptr::NonNull;

use alloc::{boxed::Box, vec::Vec};
use futures::{FutureExt, future::LocalBoxFuture};

pub mod xhci;

use crate::err::*;
pub use xhci::Xhci;

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

    pub async fn probe(&mut self) -> Result<Vec<Box<dyn Slot>>> {
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
    fn init(&mut self) -> LocalBoxFuture<'_, Result>;

    fn test_cmd(&mut self) -> LocalBoxFuture<'_, Result> {
        async { Ok(()) }.boxed_local()
    }

    fn probe(&mut self) -> LocalBoxFuture<'_, Result<Vec<Box<dyn Slot>>>>;

    fn handle_irq(&mut self) {}
}

pub trait Slot: Send {}
