use std::{sync::Arc, thread};

use futures::FutureExt;

use crate::backend::BackendOp;

#[macro_use]
mod err;

mod context;
mod device;
mod endpoint;

pub struct Libusb {
    ctx: Arc<context::Context>,
}

impl Libusb {
    pub fn new() -> Self {
        Self {
            ctx: context::Context::new().expect("Failed to create libusb context"),
        }
    }

    async fn device_list(
        &mut self,
    ) -> Result<Vec<Box<dyn super::ty::DeviceInfoOp>>, usb_if::host::USBError> {
        let ctx = self.ctx.clone();
        let devices = ctx.device_list()?;
        let mut infos = Vec::new();
        for dev in devices {
            let info = device::DeviceInfo::new(dev)?;
            infos.push(Box::new(info) as Box<dyn super::ty::DeviceInfoOp>);
        }
        Ok(infos)
    }

    async fn _open_device(
        &mut self,
        dev: &dyn super::ty::DeviceInfoOp,
    ) -> Result<Box<dyn super::ty::DeviceOp>, usb_if::host::USBError> {
        let dev_info = (dev as &dyn core::any::Any)
            .downcast_ref::<device::DeviceInfo>()
            .unwrap();

        let device = device::Device::new(dev_info, self.ctx.clone())?;
        Ok(Box::new(device) as Box<dyn super::ty::DeviceOp>)
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
            trace!("Libusb event handling thread started");
            while let Some(ctx) = handle.upgrade() {
                if let Err(e) = ctx.handle_events() {
                    error!("Libusb handle events error: {:?}", e);
                }

                trace!("Libusb event handling iteration complete");
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
        async move { self.device_list().await }.boxed()
    }

    fn open_device<'a>(
        &'a mut self,
        dev: &'a dyn super::ty::DeviceInfoOp,
    ) -> futures::future::LocalBoxFuture<
        'a,
        Result<Box<dyn super::ty::DeviceOp>, usb_if::host::USBError>,
    > {
        async move { self._open_device(dev).await }.boxed_local()
    }

    fn create_event_handler(&mut self) -> Box<dyn super::ty::EventHandlerOp> {
        panic!("Libusb does not have event handler support");
    }
}
