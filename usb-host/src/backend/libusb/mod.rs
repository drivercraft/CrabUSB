macro_rules! usb {
    ($e:expr) => {
        unsafe {
            let res = $e;
            if res >= 0 {
                Ok(res)
            } else {
                Err(crate::err::USBError::Other(
                    format!("libusb error: {:?}", res).into(),
                ))
            }
        }
    };
}

mod context;
mod device;
mod endpoint;
mod interface;
mod queue;

#[macro_use]
pub(crate) mod err;

use std::sync::Arc;

pub use device::DeviceInfo;
use futures::FutureExt;
use usb_if::host::Controller;

pub struct Libusb {
    ctx: Arc<context::Context>,
}

impl Controller for Libusb {
    fn init(&mut self) -> futures::future::LocalBoxFuture<'_, Result<(), usb_if::host::USBError>> {
        async move { Ok(()) }.boxed_local()
    }

    fn device_list(
        &self,
    ) -> futures::future::LocalBoxFuture<
        '_,
        Result<Vec<Box<dyn usb_if::host::DeviceInfo>>, usb_if::host::USBError>,
    > {
        async move {
            let list = self.ctx.device_list()?;
            let devices = list
                .map(|raw| {
                    Box::new(DeviceInfo::new(raw, self.ctx.clone()))
                        as Box<dyn usb_if::host::DeviceInfo>
                })
                .collect();

            Ok(devices)
        }
        .boxed_local()
    }

    fn handle_event(&mut self) {
        if let Err(e) = self.ctx.handle_events() {
            log::error!("Failed to handle libusb events: {e:?}");
        }
    }
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
