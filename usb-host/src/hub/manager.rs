//! Hub 设备管理器
//!
//! 管理所有 Hub 设备，包括 Root Hub 和 External Hub，维护 Hub 树结构。

use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;
use core::ops::RangeInclusive;

use usb_if::host::{
    USBError,
    hub::{HubDescriptor, PortStatus},
};

use super::event::HubId;
use crate::{Device, backend::DeviceId};

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
    /// 创建新的 Hub 设备
    pub fn new(parent_hub: Option<HubId>, depth: u8, dev: Device) -> Self {
        Self {
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

    pub async fn init(&mut self) -> Result<(), USBError> {
        self.data.dev.init().await?;

        let config = self
            .data
            .dev
            .configurations()
            .first()
            .ok_or(USBError::Other(anyhow!("Hub device has no configuration")))?
            .configuration_value;

        self.data.dev.set_configuration(config).await?;

        let descriptor = self.get_hub_descriptor().await?;
        self.data.num_ports = descriptor.num_ports;
        self.data.ports = (1..=descriptor.num_ports).map(Port::new).collect();
        self.data.descriptor = Some(descriptor);

        Ok(())
    }

    async fn get_hub_descriptor(&mut self) -> Result<HubDescriptor, USBError> {
        let mut buf = [0u8; 256];
        let len = self
            .data
            .dev
            .ep_ctrl()
            .get_descriptor(
                usb_if::descriptor::DescriptorType::HUB,
                0,
                self.data.dev.lang_id().into(),
                &mut buf,
            )
            .await?;
        HubDescriptor::from_bytes(&buf[..len])
            .ok_or(USBError::Other(anyhow!("Failed to parse hub descriptor")))
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
