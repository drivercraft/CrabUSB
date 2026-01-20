//! Hub 设备管理器
//!
//! 管理所有 Hub 设备，包括 Root Hub 和 External Hub，维护 Hub 树结构。

use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;

use crate::backend::DeviceId;

use super::device::HubDevice;
use super::event::HubId;

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
