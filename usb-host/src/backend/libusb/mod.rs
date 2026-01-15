use std::{
    sync::{Arc, Weak},
    thread,
};

use futures::FutureExt;

use crate::backend::{BackendOp, ty::EventHandlerOp};

#[macro_use]
mod err;

mod context;
mod device;

pub struct Libusb {
    ctx: Arc<context::Context>,
}

impl Libusb {
    pub fn new() -> Self {
        Self {
            ctx: context::Context::new().expect("Failed to create libusb context"),
        }
    }
}

impl Default for Libusb {
    fn default() -> Self {
        Self::new()
    }
}

impl BackendOp for Libusb {
    fn init<'a>(
        &'a mut self,
    ) -> futures::future::BoxFuture<'a, Result<(), usb_if::host::USBError>> {
        let handle = Arc::downgrade(&self.ctx);

        thread::spawn(move || {
            while let Some(ctx) = handle.upgrade() {
                if let Err(e) = ctx.handle_events() {
                    error!("Libusb handle events error: {:?}", e);
                }
            }
        });

        async { Ok(()) }.boxed()
    }

    fn probe_devices<'a>(
        &'a mut self,
    ) -> futures::future::BoxFuture<
        'a,
        Result<Vec<Box<dyn super::ty::DeviceInfoOp>>, usb_if::host::USBError>,
    > {
        todo!()
    }

    fn open_device<'a>(
        &'a mut self,
        dev: &'a dyn super::ty::DeviceInfoOp,
    ) -> futures::future::LocalBoxFuture<
        'a,
        Result<Box<dyn super::ty::DeviceOp>, usb_if::host::USBError>,
    > {
        todo!()
    }

    fn create_event_handler(&mut self) -> Box<dyn super::ty::EventHandlerOp> {
        panic!("Libusb does not have event handler support");
    }
}
