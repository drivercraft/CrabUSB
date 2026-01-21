use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};

use futures::{
    FutureExt,
    future::{BoxFuture, LocalBoxFuture},
};
use id_arena::{Arena, Id};
use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor};

use crate::{
    Device,
    backend::{
        BackendOp, CoreOp,
        ty::{DeviceInfoOp, DeviceOp, EventHandlerOp},
    },
    hub::{DeviceAddressInfo, Hub, HubDevice},
};

pub struct Core {
    pub(crate) backend: Box<dyn CoreOp>,
    hubs: Arena<Hub>,
    root_hub: Option<Id<Hub>>,
    inited_devices: BTreeMap<usize, Box<dyn DeviceOp>>,
}

impl Core {
    pub(crate) fn new(backend: impl CoreOp) -> Self {
        Self {
            root_hub: None,
            backend: Box::new(backend),
            hubs: Arena::new(),
            inited_devices: BTreeMap::new(),
        }
    }

    async fn _probe_devices(
        &mut self,
    ) -> Result<(bool, Vec<Box<dyn DeviceInfoOp>>), usb_if::host::Error> {
        let mut is_have_new_hub = false;
        let mut out = Vec::new();

        let hub_ids: Vec<Id<Hub>> = self.hubs.iter().map(|(id, _)| id).collect();

        for id in hub_ids {
            let addr_infos = self.hub_changed_ports(id).await?;

            for addr_info in addr_infos {
                debug!(
                    "Found device at hub {:?}, port {}",
                    id, addr_info.root_port_id
                );

                let device = self.backend.new_addressed_device(addr_info.clone()).await?;

                let device_id = device.id();

                if let Some(hub_settings) =
                    HubDevice::is_hub(device.descriptor(), device.configuration_descriptors())
                {
                    info!("Found hub device at hub {:?}", id);

                    let device_inner: Device = device.into();

                    let mut hub_device = HubDevice::new(device_inner, hub_settings).await?;
                    hub_device.init().await?;
                    let mut hub = Hub::new(Box::new(hub_device));
                    hub.setup(id);
                    let hub_id = self.hubs.alloc(hub);
                    is_have_new_hub = true;

                    info!("Added new hub with id {:?}", hub_id);
                } else {
                    let desc = device.descriptor().clone();
                    let configs = device.configuration_descriptors().to_vec();

                    self.inited_devices.insert(device_id, device);

                    let device_info = Box::new(DeviceInfo::new(device_id, desc, &configs))
                        as Box<dyn DeviceInfoOp>;

                    out.push(device_info);
                }
            }
        }

        Ok((is_have_new_hub, out))
    }

    async fn hub_changed_ports(
        &mut self,
        hub_id: Id<Hub>,
    ) -> Result<Vec<DeviceAddressInfo>, usb_if::host::Error> {
        let hub = self.hubs.get_mut(hub_id).expect("Hub id should be valid");
        hub.backend.changed_ports().await
    }

    async fn probe_devices(&mut self) -> Result<Vec<Box<dyn DeviceInfoOp>>, usb_if::host::Error> {
        let mut result = Vec::new();

        loop {
            let (is_have_new_hub, mut devices) = self._probe_devices().await?;
            result.append(&mut devices);
            if !is_have_new_hub {
                break;
            }
        }
        Ok(result)
    }
}

impl BackendOp for Core {
    fn init<'a>(&'a mut self) -> BoxFuture<'a, Result<(), usb_if::host::Error>> {
        async {
            self.backend.init().await?;
            let mut root_hub = Hub::new(self.backend.root_hub());
            root_hub.backend.reset()?;

            let id = self.hubs.alloc(root_hub);
            self.root_hub = Some(id);
            Ok(())
        }
        .boxed()
    }

    fn device_list<'a>(
        &'a mut self,
    ) -> BoxFuture<'a, Result<Vec<Box<dyn DeviceInfoOp>>, usb_if::host::Error>> {
        self.probe_devices().boxed()
    }

    fn open_device<'a>(
        &'a mut self,
        dev: &'a dyn crate::backend::ty::DeviceInfoOp,
    ) -> LocalBoxFuture<'a, Result<Box<dyn DeviceOp>, usb_if::host::Error>> {
        async {
            let device = self.inited_devices.remove(&dev.id()).unwrap_or_else(|| {
                panic!("Device id {} not found in inited_devices", dev.id());
            });

            Ok(device)
        }
        .boxed()
    }

    fn create_event_handler(&mut self) -> Box<dyn EventHandlerOp> {
        self.backend.create_event_handler()
    }
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    id: usize,
    desc: DeviceDescriptor,
    config_desc: Vec<ConfigurationDescriptor>,
}

impl DeviceInfo {
    pub fn new(id: usize, desc: DeviceDescriptor, config_desc: &[ConfigurationDescriptor]) -> Self {
        Self {
            id,
            desc,
            config_desc: config_desc.to_vec(),
        }
    }
}

impl DeviceInfoOp for DeviceInfo {
    fn id(&self) -> usize {
        self.id
    }

    fn backend_name(&self) -> &str {
        "kernel"
    }

    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor] {
        &self.config_desc
    }
}
