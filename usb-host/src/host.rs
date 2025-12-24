use alloc::sync::Arc;
use alloc::sync::Weak;
use core::cell::UnsafeCell;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

use usb_if::host::USBError;

use crate::Mmio;
use crate::backend::ty::*;

pub use crate::backend::xhci::Xhci;

// ============================================================================
// UsbHost 包装器 (线程安全)
// ============================================================================

/// 后端包装器 (用于内部可变性)
struct BackendWrapper<B>(UnsafeCell<B>);

unsafe impl<B> Send for BackendWrapper<B> {}

/// USB 主机控制器
///
/// 提供线程安全的 USB 主机控制器访问，支持事件处理
pub struct USBHost<B> {
    /// 事件处理器使用标志
    handler_used: Arc<AtomicBool>,

    /// 后端控制器
    backend: Arc<BackendWrapper<B>>,
}

impl USBHost<Xhci> {
    pub fn new_xhci(mmio: Mmio, dma_mask: usize) -> USBHost<Xhci> {
        USBHost::new(Xhci::new(mmio, dma_mask))
    }
}

impl<B: HostOp> USBHost<B> {
    /// 创建新的 USB 主机控制器
    pub(crate) fn new(backend: B) -> Self {
        Self {
            backend: Arc::new(BackendWrapper(UnsafeCell::new(backend))),
            handler_used: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 初始化主机控制器
    pub async fn init(&mut self) -> Result<(), USBError> {
        // Safety: 初始化期间独占访问后端
        let backend = unsafe { &mut *self.backend.0.get() };
        backend.initialize().await
    }

    /// 创建事件处理器
    ///
    /// # Panics
    /// 如果已经创建了事件处理器，则会 panic
    pub fn event_handler(&mut self) -> EventHandler<B> {
        self.handler_used
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .expect("Only one EventHandler can be created from UsbHost");

        EventHandler {
            backend: Arc::downgrade(&self.backend),
            used: self.handler_used.clone(),
        }
    }
}

/// 事件处理器
///
/// 用于在中断上下文中处理 USB 事件
pub struct EventHandler<B> {
    used: Arc<AtomicBool>,
    backend: Weak<BackendWrapper<B>>,
}

unsafe impl<B> Send for EventHandler<B> {}
unsafe impl<B> Sync for EventHandler<B> {}

impl<B: HostOp> EventHandler<B> {
    /// 处理 USB 事件
    ///
    /// # Safety
    /// 必须在中断上下文中调用
    pub fn handle_events(&self) -> Result<(), USBError> {
        let backend = match self.backend.upgrade() {
            Some(b) => b,
            None => {
                // UsbHost 已被释放
                return Ok(());
            }
        };

        // Safety: EventHandler 确保同一时间只有一个线程调用 poll_events
        let backend = unsafe { &mut *backend.0.get() };
        backend.poll_events();
        Ok(())
    }
}

impl<B> Drop for EventHandler<B> {
    fn drop(&mut self) {
        self.used.store(false, core::sync::atomic::Ordering::SeqCst);
    }
}
