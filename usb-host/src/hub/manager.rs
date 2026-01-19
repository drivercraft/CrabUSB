//! Hub 设备管理器
//!
//! 管理所有 Hub 设备，包括 Root Hub 和 External Hub，维护 Hub 树结构。

use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;

use usb_if::{
    descriptor::{Class, EndpointType},
    host::{
        ControlSetup, USBError,
        hub::{HubDescriptor, PortStatus},
    },
    transfer::{Recipient, Request, RequestType},
};

use super::event::HubId;
use crate::{Device, DeviceInfo, backend::DeviceId};

/// Hub 设备管理器
///
/// 维护所有 Hub 设备的树形结构，处理设备枚举和事件。
pub struct HubManager {
    /// Hub 树 (hub_id -> HubDevice)
    hubs: BTreeMap<HubId, HubDevice>,

    /// 设备到 Hub 的映射 (device_id -> hub_id)
    device_to_hub: BTreeMap<DeviceId, HubId>,
}

impl HubManager {
    /// 创建新的 Hub 管理器
    pub fn new() -> Self {
        Self {
            hubs: BTreeMap::new(),
            device_to_hub: BTreeMap::new(),
        }
    }

    /// 添加 Hub 设备
    pub fn add_hub(&mut self, hub: HubDevice) -> HubId {
        let hub_id = hub.id();
        self.hubs.insert(hub_id, hub);
        hub_id
    }

    /// 移除 Hub 设备
    pub fn remove_hub(&mut self, hub_id: HubId) -> Option<HubDevice> {
        self.hubs.remove(&hub_id)
    }

    /// 获取 Hub 设备
    pub fn get_hub(&self, hub_id: HubId) -> Option<&HubDevice> {
        self.hubs.get(&hub_id)
    }

    /// 获取 Hub 设备（可变）
    pub fn get_hub_mut(&mut self, hub_id: HubId) -> Option<&mut HubDevice> {
        self.hubs.get_mut(&hub_id)
    }

    /// 注册设备到 Hub
    pub fn register_device(&mut self, device_id: DeviceId, hub_id: HubId) {
        self.device_to_hub.insert(device_id, hub_id);
    }

    /// 注销设备
    pub fn unregister_device(&mut self, device_id: DeviceId) -> Option<HubId> {
        self.device_to_hub.remove(&device_id)
    }

    /// 获取设备所属的 Hub
    pub fn get_device_hub(&self, device_id: DeviceId) -> Option<HubId> {
        self.device_to_hub.get(&device_id).copied()
    }

    /// 获取所有 Hub ID
    pub fn hub_ids(&self) -> Vec<HubId> {
        self.hubs.keys().copied().collect()
    }

    /// 获取 Hub 数量
    pub fn hub_count(&self) -> usize {
        self.hubs.len()
    }
}

impl Default for HubManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Hub 设备
///
/// 表示一个 Hub 设备（Root Hub 或 External Hub）。
pub struct HubDevice {
    config: u8,
    interface: u8,
    data: Box<Inner>,
}

struct Inner {
    /// Hub 状态
    pub state: HubState,

    /// 端口数量
    pub num_ports: u8,

    /// 端口列表
    pub ports: Vec<Port>,

    pub parent_hub: Option<HubId>,

    /// 层级深度（Root Hub = 0）
    pub depth: u8,

    pub dev: Device,

    pub descriptor: Option<HubDescriptor>,
}

impl HubDevice {
    /// returns (config_value, interface_number) if the device is a hub
    pub fn is_hub(info: &DeviceInfo) -> Option<(u8, u8)> {
        if !matches!(info.descriptor().class(), Class::Hub(_)) {
            return None;
        }
        let Some(config) = info.configurations().get(0) else {
            warn!("Hub device has no configurations");
            return None;
        };

        for interface in &config.interfaces {
            for alt in &interface.alt_settings {
                if alt.subclass != 0x00 && alt.protocol != 0x00 {
                    continue;
                }

                if alt.num_endpoints != 1 {
                    continue;
                }

                if alt.endpoints[0].transfer_type != EndpointType::Interrupt
                    || alt.endpoints[0].direction != usb_if::transfer::Direction::In
                {
                    continue;
                }

                return Some((config.configuration_value, interface.interface_number));
            }
        }

        None
    }

    /// 创建新的 Hub 设备
    pub fn new(
        parent_hub: Option<HubId>,
        depth: u8,
        dev: Device,
        config: u8,
        interface: u8,
    ) -> Self {
        Self {
            config,
            interface,
            data: Box::new(Inner {
                state: HubState::Uninitialized,
                num_ports: 0,
                ports: vec![],
                parent_hub,
                depth,
                dev,
                descriptor: None,
            }),
        }
    }

    pub fn id(&self) -> HubId {
        self.data.as_ref() as *const Inner as usize as HubId
    }

    pub fn is_superspeed(&self) -> bool {
        self.data.dev.descriptor().protocol == 3
    }

    pub async fn init(&mut self) -> Result<(), USBError> {
        // 第一阶段：设备初始化（参考 Linux hub_configure 前半部分）
        self.data.dev.init().await?;

        self.data.dev.set_configuration(self.config).await?;

        // 第二阶段：获取 Hub 描述符（带重试）
        let descriptor = self.get_hub_descriptor().await?;
        self.data.descriptor = Some(descriptor);
        if self.hub_descriptor().bNbrPorts == 0 {
            return Err(USBError::Other(anyhow!("Hub has zero ports")));
        }
        self.data.num_ports = self.hub_descriptor().bNbrPorts;

        // 第三阶段：初始化端口状态（参考 Linux hub_activate）
        // 初始化所有端口为 Disconnected 状态
        self.data.ports = (1..=self.data.num_ports).map(Port::new).collect();

        self.data.dev.claim_interface(self.interface, 0).await?;

        // TODO: 实现端口电源开启（Linux: hub_power_on）
        // 注意：QEMU 等模拟环境可能不支持 SetPortFeature 命令
        // 需要添加超时机制避免卡死

        // TODO: 实现端口状态检测（Linux: hub_init_func2/3）
        // 需要轮询端口状态变化，检测设备连接

        // 标记 Hub 为运行状态
        self.data.state = HubState::Running;
        debug!("Hub initialized with {} ports", self.data.num_ports);
        Ok(())
    }

    fn hub_descriptor(&self) -> &HubDescriptor {
        self.data.descriptor.as_ref().unwrap()
    }

    /// 获取 Hub 描述符（参考 Linux 内核实现）
    ///
    /// Linux 内核位置: drivers/usb/core/hub.c:get_hub_descriptor()
    ///
    /// 重试策略:
    /// - 最多重试 3 次
    /// - 使用小缓冲区（USB 2.0 Hub 描述符可变长）
    /// - QEMU 等模拟环境可能不支持，使用默认值
    async fn get_hub_descriptor(&mut self) -> Result<HubDescriptor, USBError> {
        const DT_SS_HUB: u16 = 0x0a;
        const DT_HUB: u16 = 0x9;

        let dtype;
        let size;

        if self.is_superspeed() {
            dtype = DT_SS_HUB;
            size = 12;
        } else {
            dtype = DT_HUB;
            size = size_of::<HubDescriptor>();
        }

        let mut buff = vec![0u8; size];

        const MAX_RETRIES: u8 = 3;

        // 参考 Linux 的重试机制
        for attempt in 1..=MAX_RETRIES {
            let result = self
                .data
                .dev
                .ep_ctrl()
                .control_in(
                    ControlSetup {
                        request_type: RequestType::Class,
                        recipient: Recipient::Device,
                        request: Request::GetDescriptor,
                        value: dtype << 8,
                        index: 0,
                    },
                    &mut buff,
                )
                .await;

            let desc = unsafe { *(buff.as_ptr() as *const HubDescriptor) };

            match result {
                Ok(act_size) => {
                    if self.is_superspeed() {
                        if act_size == 12 {
                            return Ok(desc);
                        }
                    } else if act_size >= 9 {
                        let size = 7 + desc.bNbrPorts / 8 + 1;
                        if (act_size as u8) < size {
                            return Err(USBError::Other(anyhow!("Hub descripoter size error")));
                        }
                        return Ok(desc);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to get hub descriptor on attempt {}: {:?}",
                        attempt, e
                    );
                }
            }
        }

        Err(USBError::Other(anyhow!("Hub get descriptor failed")))
    }
}

/// Hub 状态
#[derive(Debug)]
pub enum HubState {
    /// 未初始化
    Uninitialized,

    /// 初始化中（stage: 1/2/3）
    Initializing { stage: u8 },

    /// 运行中
    Running,

    /// 挂起
    Suspended,

    /// 错误状态
    Error(USBError),
}

/// 端口
pub struct Port {
    /// 端口号（1-based）
    pub index: u8,

    /// 端口状态
    pub status: PortStatus,

    /// 端口状态机
    pub state: PortState,

    /// 连接的设备
    pub connected_device: Option<DeviceId>,

    /// 是否需要 Transaction Translator
    pub tt_required: bool,
}

impl Port {
    /// 创建新端口
    pub fn new(index: u8) -> Self {
        Self {
            index,
            status: PortStatus {
                connected: false,
                enabled: false,
                suspended: false,
                over_current: false,
                resetting: false,
                powered: false,
                low_speed: false,
                high_speed: false,
                speed: usb_if::host::hub::DeviceSpeed::Full,
                change: usb_if::host::hub::PortStatusChange {
                    connection_changed: false,
                    enabled_changed: false,
                    reset_complete: false,
                    suspend_changed: false,
                    over_current_changed: false,
                },
            },
            state: PortState::Disconnected,
            connected_device: None,
            tt_required: false,
        }
    }
}

/// 端口状态机
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    /// 断电
    PoweredOff,

    /// 未连接
    Disconnected,

    /// 复位中
    Resetting,

    /// 已使能
    Enabled,

    /// 挂起
    Suspended,

    /// 禁用
    Disabled,

    /// 过流
    OverCurrent,
}
