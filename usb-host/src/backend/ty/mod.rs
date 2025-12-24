use alloc::sync::{Arc, Weak};
use core::{cell::UnsafeCell, sync::atomic::AtomicBool};

use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor};

use crate::err::USBError;

pub(crate) trait Backend: Send + 'static {
    type Device: Device;

    async fn initialize(&mut self) -> Result<(), USBError>;
    async fn device_list(&self) -> Result<Vec<DeviceDescriptor>, USBError>;

    async fn open_device(&mut self, desc: &DeviceDescriptor) -> Result<Self::Device, USBError>;

    fn poll_events(&mut self);
}

pub(crate) trait Device: Send + 'static {
    fn descriptor(&self) -> Result<DeviceDescriptor, USBError>;

    async fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> Result<Box<dyn usb_if::host::Interface>, USBError>;

    fn control_in<'a>(&mut self, setup: ControlSetup, data: &'a mut [u8]) -> ResultTransfer<'a>;
    fn control_out<'a>(&mut self, setup: ControlSetup, data: &'a [u8]) -> ResultTransfer<'a>;
}

struct BackendWarper<B>(UnsafeCell<B>);

unsafe impl<B> Send for BackendWarper<B> {}

pub struct UsbHost<B> {
    handler_used: Arc<AtomicBool>,
    backend: Arc<BackendWarper<B>>,
}

impl<B: Backend> UsbHost<B> {
    pub(crate) fn new(backend: B) -> Self {
        Self {
            backend: Arc::new(BackendWarper(UnsafeCell::new(backend))),
            handler_used: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn initialize(&mut self) -> Result<(), USBError> {
        // Safety: We have exclusive access to the backend during initialization.
        let backend = unsafe { &mut *self.backend.0.get() };
        backend.initialize().await
    }

    pub fn event_handler(&mut self) -> EventHandler<B> {
        self.handler_used
            .compare_exchange(
                false,
                true,
                core::sync::atomic::Ordering::SeqCst,
                core::sync::atomic::Ordering::SeqCst,
            )
            .expect("Only one EventHandler can be created from UsbHost");

        EventHandler {
            backend: Arc::downgrade(&self.backend),
            used: self.handler_used.clone(),
        }
    }
}

pub struct EventHandler<B> {
    used: Arc<AtomicBool>,
    backend: Weak<BackendWarper<B>>,
}

unsafe impl<B> Send for EventHandler<B> {}
unsafe impl<B> Sync for EventHandler<B> {}

impl<B: Backend> EventHandler<B> {
    pub fn handle_events(&mut self) -> Result<(), USBError> {
        let backend = match self.backend.upgrade() {
            Some(b) => b,
            None => {
                return Ok(());
            }
        };

        // Safety: The EventHandler ensures that only one thread can call poll_events at a time.
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
