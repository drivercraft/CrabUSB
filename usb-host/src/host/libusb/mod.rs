use crate::{Controller, Host};

macro_rules! usb {
    ($e:expr) => {
        unsafe {
            let res = $e;
            if res >= 0 {
                Ok(res)
            } else {
                Err(crate::err::USBError::Unknown)
            }
        }
    };
}

mod context;
mod device;

#[macro_use]
pub(crate) mod err;

pub use device::DeviceInfo;

pub struct Libusb {
    ctx: context::Context,
}

impl Controller for Libusb {
    type Device = device::DeviceInfo;

    async fn init(&mut self) -> crate::err::Result {
        Ok(())
    }

    async fn test_cmd(&mut self) -> crate::err::Result {
        Ok(())
    }

    async fn probe(&mut self) -> crate::err::Result<Vec<Self::Device>> {
        let ls = self.ctx.device_list()?;
        Ok(ls.map(device::DeviceInfo::new).collect())
    }
}

impl Host<Libusb> {
    pub fn new_libusb() -> Self {
        Self {
            ctrl: Libusb {
                ctx: context::Context::new().expect("Failed to create libusb context"),
            },
        }
    }
}
