use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, vec_deque::VecDeque},
    vec::Vec,
};

use futures::{FutureExt, future::BoxFuture};
use usb_if::descriptor::{ConfigurationDescriptor, DeviceDescriptor};

use crate::{
    backend::{
        BackendOp, CoreOp,
        ty::{DeviceInfoOp, DeviceOp, HubOp},
    },
    hub::{HubDevice, RouteString},
};

pub struct Core {
    pub(crate) backend: Box<dyn CoreOp>,
    root_hub: Option<Box<dyn HubOp>>,
    device_hubs: Vec<Box<dyn HubOp>>,
    inited_devices: BTreeMap<usize, Box<dyn DeviceOp>>,
}

impl Core {
    pub(crate) fn new(mut backend: impl CoreOp) -> Self {
        let root_hub = Some(backend.root_hub());
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
        use alloc::collections::vec_deque::VecDeque;

        let mut result: Vec<Box<dyn DeviceInfoOp>> = Vec::new();

        // 遍历栈: 存储(父hub, 路由字符串)
        let mut hub_stack: VecDeque<(Box<dyn HubOp>, RouteString)> = VecDeque::new();

        // 当前正在扫描的hub和路由
        let mut current_hub: Option<Box<dyn HubOp>> = self.root_hub.take();
        let mut current_route = RouteString::follow_root();

        // 深度优先遍历
        loop {
            // 获取当前hub
            let mut hub = match current_hub.take() {
                Some(h) => h,
                None => break,
            };

            // 获取端口上的设备
            let addr_infos = match hub.changed_ports().await {
                Ok(devs) => devs,
                Err(e) => {
                    warn!(
                        "Failed to get changed ports at route {:?}: {:?}",
                        current_route, e
                    );
                    // 尝试返回父hub
                    match hub_stack.pop_back() {
                        Some((parent_hub, parent_route)) => {
                            current_hub = Some(parent_hub);
                            current_route = parent_route;
                            continue;
                        }
                        None => break,
                    }
                }
            };

            let mut found_child_hub = false;

            // 处理每个设备
            for addr_info in addr_infos {
                debug!(
                    "Found device at route {:?}, port {}",
                    current_route, addr_info.root_port_id
                );

                // 保存端口号,因为addr_info会被移动
                let port_number = addr_info.root_port_id;

                // 通过backend创建设备
                let device = match self.backend.new_addressed_device(addr_info).await {
                    Ok(dev) => dev,
                    Err(e) => {
                        warn!(
                            "Failed to create device at route {:?}: {:?}",
                            current_route, e
                        );
                        continue;
                    }
                };

                let device_id = device.id();

                // 判断是否为Hub (在移动device之前)
                let is_hub =
                    HubDevice::is_hub(device.descriptor(), device.configuration_descriptors());

                if let Some(hub_settings) = is_hub {
                    info!("Found hub device at route {:?}", current_route);

                    // 转换Device为HubDevice
                    let device_inner: crate::Device = device.into();

                    match HubDevice::new(device_inner, hub_settings).await {
                        Ok(mut hub_device) => {
                            // 初始化Hub
                            if let Err(e) = hub_device.init().await {
                                warn!("Failed to init hub at route {:?}: {:?}", current_route, e);
                                continue;
                            }

                            // 更新路由字符串
                            let parent_route = current_route.clone();
                            current_route.push_hub(port_number);

                            info!("Entering hub at route {:?}", current_route);

                            // 保存当前hub到栈
                            hub_stack.push_back((hub, parent_route));

                            // 设置新hub为当前hub
                            current_hub = Some(Box::new(hub_device));
                            found_child_hub = true;

                            // 跳出设备循环,处理新hub
                            break;
                        }
                        Err(e) => {
                            warn!("Failed to create hub device: {:?}", e);
                            continue;
                        }
                    }
                } else {
                    // 普通设备,需要先获取描述符再移动device
                    let desc = device.descriptor().clone();
                    let configs = device.configuration_descriptors().to_vec();

                    // 添加到inited_devices
                    self.inited_devices.insert(device_id, device);

                    // 创建设备信息
                    let device_info = Box::new(DeviceInfo::new(device_id, desc, &configs))
                        as Box<dyn DeviceInfoOp>;

                    result.push(device_info);
                }
            }

            // 如果没有发现子hub,尝试返回父hub
            if !found_child_hub {
                match hub_stack.pop_back() {
                    Some((parent_hub, parent_route)) => {
                        debug!("Returning to parent hub");
                        current_hub = Some(parent_hub);
                        current_route = parent_route;
                    }
                    None => {
                        // 栈为空,遍历完成
                        break;
                    }
                }
            }
        }

        info!("Probe complete, found {} devices", result.len());
        Ok(result)
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
            if let Some(ref mut hub) = self.root_hub {
                hub.reset()?;
            }
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
