//! Hub 事件处理
//!
//! 定义 Hub 相关的事件类型和事件处理器。

use alloc::vec::Vec;
use core::task::Waker;
use futures::task::AtomicWaker;

use crossbeam::queue::SegQueue;
use usb_if::host::hub::PortStatusChange;

use crate::backend::DeviceId;

/// Hub 事件
#[derive(Debug)]
pub enum HubEvent {
    /// 端口状态变化
    PortChange {
        hub_id: HubId,
        port_index: u8,
        change: PortStatusChange,
    },

    /// Hub 状态变化
    HubStatusChange {
        hub_id: HubId,
        status: HubStatusChange,
    },

    /// 设备连接
    DeviceConnected {
        hub_id: HubId,
        port_index: u8,
        device_id: DeviceId,
    },

    /// 设备断开
    DeviceDisconnected { hub_id: HubId, port_index: u8 },
}

/// Hub 状态变化（参照 USB 2.0 规范表 11-19）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HubStatusChange {
    /// 局部电源丢失
    pub local_power_lost: bool,

    /// 过流条件
    pub over_current: bool,
}

/// Hub ID 类型
pub type HubId = u32;

/// Hub 事件处理器
///
/// 使用无锁队列处理 Hub 事件，支持异步事件处理。
pub struct HubEventHandler {
    /// 事件队列（无锁）
    events: SegQueue<HubEvent>,

    /// 事件通知
    waker: AtomicWaker,
}

impl HubEventHandler {
    /// 创建新的事件处理器
    pub fn new() -> Self {
        Self {
            events: SegQueue::new(),
            waker: AtomicWaker::new(),
        }
    }

    /// 推送事件到队列
    pub fn push(&self, event: HubEvent) {
        self.events.push(event);
        self.waker.wake();
    }

    /// 检查是否有待处理事件
    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    /// 弹出所有待处理事件
    pub fn drain(&self) -> Vec<HubEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.events.pop() {
            events.push(event);
        }
        events
    }

    /// 注册 Waker
    pub fn register(&self, waker: &Waker) {
        self.waker.register(waker);
    }
}

impl Default for HubEventHandler {
    fn default() -> Self {
        Self::new()
    }
}
