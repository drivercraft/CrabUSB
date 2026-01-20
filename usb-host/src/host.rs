use alloc::boxed::Box;
use alloc::vec::Vec;
use usb_if::descriptor::{Class, HubSpeed};

use crate::backend::{CoreOp, ty::*};
use crate::err::Result;
use crate::hub::{HubDevice, HubId};
use crate::kcore::*;
use crate::{Mmio, backend::BackendOp};

pub use super::backend::{
    dwc::{
        CruOp, Dwc, DwcNewParams, DwcParams, UdphyParam, Usb2PhyParam, Usb2PhyPortId,
        UsbPhyInterfaceMode,
    },
    ty::ep::*,
    xhci::Xhci,
};
pub use crate::device::{Device, DeviceInfo};

/// USB 主机控制器
pub struct USBHost {
    backend: Box<dyn BackendOp>,
}

impl USBHost {
    pub fn new_xhci(mmio: Mmio, dma_mask: usize) -> Result<USBHost> {
        Ok(USBHost::new(Xhci::new(mmio, dma_mask)?))
    }

    pub fn new_dwc(params: DwcNewParams<'_, impl CruOp>) -> Result<USBHost> {
        Ok(USBHost::new(Dwc::new(params)?))
    }

    #[cfg(libusb)]
    pub fn new_libusb() -> Result<USBHost> {
        let host = USBHost::new_user(crate::backend::libusb::Libusb::new());
        Ok(host)
    }
}

impl USBHost {
    pub(crate) fn new(backend: impl CoreOp) -> Self {
        let b = Core::new(backend);
        Self {
            backend: Box::new(b),
        }
    }

    #[cfg(libusb)]
    pub(crate) fn new_user(backend: impl BackendOp) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }

    /// 初始化主机控制器
    pub async fn init(&mut self) -> Result<()> {
        self.backend.init().await?;
        Ok(())
    }

    pub async fn probe_devices(&mut self) -> Result<Vec<DeviceInfo>> {
        let device_infos = self.backend.device_list().await?;
        let mut devices = Vec::new();
        for dev in device_infos {
            let dev_info = DeviceInfo { inner: dev };
            devices.push(dev_info);
        }
        Ok(devices)
    }

    async fn probe_handle_hub(
        &mut self,
        info: DeviceInfo,
        speed: HubSpeed,
        config: u8,
        interface: u8,
    ) -> Result<Vec<DeviceInfo>> {
        debug!("Found hub: {:?}, speed: {:?}", info, speed);

        // 待处理的 Hub 栈，用于支持多级 Hub
        let mut hub_stack: Vec<HubStack> = vec![];

        hub_stack.push(HubStack {
            info,
            hub_speed: speed,
            parent_hub: None,
            config,
            interface,
            depth: 0,
        });

        // 最终返回的非 Hub 设备列表
        let mut non_hub_devices = Vec::new();

        // 循环处理栈中的 Hub
        while let Some(stack) = hub_stack.pop() {
            debug!(
                "Processing hub at depth {}, parent: {:?}",
                stack.depth,
                if let Some(id) = stack.parent_hub {
                    format!("HubId {:#x}", id)
                } else {
                    "Root".into()
                }
            );

            // 打开 Hub 设备
            let device = match self.open_device(&stack.info).await {
                Ok(dev) => dev,
                Err(e) => {
                    warn!("Failed to open hub device: {:?}", e);
                    continue;
                }
            };

            let mut device = HubDevice::new(
                stack.parent_hub,
                stack.depth,
                device,
                stack.config,
                stack.interface,
            )
            .await?;
            device.init().await?;

            let devices = device.probe_devices()?;
            for dev in devices {
                if let Class::Hub(speed) = dev.descriptor().class() {
                    if let Some((config, interface)) = HubDevice::is_hub(&dev) {
                        hub_stack.push(HubStack {
                            info: dev,
                            hub_speed: speed,
                            parent_hub: Some(device.id()),
                            config,
                            interface,
                            depth: stack.depth + 1,
                        });
                    } else {
                        non_hub_devices.push(dev);
                    }
                } else {
                    non_hub_devices.push(dev);
                }
            }
        }

        Ok(non_hub_devices)
    }

    pub fn create_event_handler(&mut self) -> EventHandler {
        let handler = self.backend.create_event_handler();
        EventHandler { handler }
    }

    pub async fn open_device(&mut self, dev: &DeviceInfo) -> Result<Device> {
        let device = self.backend.open_device(dev.inner.as_ref()).await?;
        let mut device: Device = device.into();
        device.init().await?;
        Ok(device)
    }
}

struct HubStack {
    info: DeviceInfo,
    hub_speed: HubSpeed,
    parent_hub: Option<HubId>,
    config: u8,
    interface: u8,
    depth: u8,
}

impl HubStack {
    fn is_root(&self) -> bool {
        self.parent_hub.is_none()
    }
}

pub struct EventHandler {
    handler: Box<dyn EventHandlerOp>,
}

impl EventHandler {
    /// 处理事件
    pub fn handle_event(&self) -> Event {
        self.handler.handle_event()
    }
}
