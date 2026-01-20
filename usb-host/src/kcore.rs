use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    vec::Vec,
};

use futures::{FutureExt, future::BoxFuture};
use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor};

use crate::{
    Device,
    backend::{
        BackendOp, CoreOp,
        ty::{DeviceInfoOp, DeviceOp, HubOp},
    },
    hub::{HubDevice, RouteString},
};

pub struct Core {
    pub(crate) backend: Box<dyn CoreOp>,
    root_hub: Box<dyn HubOp>,
    device_hubs: Vec<Box<dyn HubOp>>,
    inited_devices: BTreeMap<usize, Box<dyn DeviceOp>>,
}

struct ProbStack<'a> {
    route_string: RouteString,
    hub: &'a mut Box<dyn HubOp>,
    port_id: Option<usize>,
}

impl Core {
    pub(crate) fn new(mut backend: impl CoreOp) -> Self {
        let root_hub = backend.root_hub();
        Self {
            backend: Box::new(backend),
            root_hub,
            inited_devices: BTreeMap::new(),
            device_hubs: vec![],
        }
    }

    async fn probe_devices(
        &mut self,
    ) -> Result<alloc::vec::Vec<Box<dyn crate::backend::ty::DeviceInfoOp>>, usb_if::host::Error>
    {
        let mut out: Vec<Box<dyn DeviceInfoOp>> = Vec::new();
        let mut stack: VecDeque<ProbStack> = VecDeque::new();
        stack.push_back(ProbStack {
            route_string: RouteString::follow_root(),
            hub: &mut self.root_hub,
            port_id: None,
        });

        Ok(out)
    }

    // async fn probe_hub(
    //     backend: &mut Box<dyn CoreOp>,
    //     hub: &mut Box<dyn HubOp>,
    //     info_list: &mut Vec<Box<dyn DeviceInfoOp>>,
    //     dev_list: &mut Vec<Box<dyn DeviceOp>>,
    // ) -> Result<(), usb_if::host::Error> {
    //     let changed_ports = hub.changed_ports().await?;
    //     let mut stack = VecDeque::new();

    //     for addr_info in changed_ports {
    //         let device = backend.new_addressed_device(addr_info).await?;
    //         let device_id = device.id();

    //         if let Some(hub) =
    //             HubDevice::is_hub(device.descriptor(), device.configuration_descriptors())
    //         {
    //             let mut hub = HubDevice::new(device.into(), hub).await?;
    //             hub.init().await?;
    //             stack.push_back((device_id, hub));
    //         } else {
    //             let device_info = Box::new(DeviceInfo::new(
    //                 device_id,
    //                 device.descriptor().clone(),
    //                 device.configuration_descriptors(),
    //             )) as _;
    //             info_list.push(device_info);
    //             dev_list.push(device);
    //         }
    //     }
    //     Ok(())
    // }
}

impl BackendOp for Core {
    fn init<'a>(&'a mut self) -> BoxFuture<'a, Result<(), usb_if::host::Error>> {
        async {
            self.backend.init().await?;
            self.root_hub.reset()?;
            Ok(())
        }
        .boxed()
    }

    fn device_list<'a>(
        &'a mut self,
    ) -> BoxFuture<
        'a,
        Result<alloc::vec::Vec<Box<dyn crate::backend::ty::DeviceInfoOp>>, usb_if::host::Error>,
    > {
        self.probe_devices().boxed()
    }

    fn open_device<'a>(
        &'a mut self,
        dev: &'a dyn crate::backend::ty::DeviceInfoOp,
    ) -> futures::future::LocalBoxFuture<
        'a,
        Result<Box<dyn crate::backend::ty::DeviceOp>, usb_if::host::Error>,
    > {
        async {
            let device = self.inited_devices.remove(&dev.id()).unwrap_or_else(|| {
                panic!("Device id {} not found in inited_devices", dev.id());
            });

            Ok(device)
        }
        .boxed()
    }

    fn create_event_handler(&mut self) -> Box<dyn crate::backend::ty::EventHandlerOp> {
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
