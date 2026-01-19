//! xHCI 端口实现
//!
//! 实现 xHCI 控制器的端口操作，遵循 USB 2.0 规范 11.24。

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::time::Duration;
use futures::future::LocalBoxFuture;
use spin::RwLock;

use usb_if::host::hub::{DeviceSpeed, PortStatus, PortStatusChange};

use crate::backend::xhci::reg::XhciRegistersShared;

/// xHCI 端口
///
/// 表示 xHCI 控制器的一个物理端口。
pub struct XhciPort {
    /// 端口索引（1-based）
    pub index: u8,

    /// 端口寄存器偏移
    reg_offset: usize,

    /// 缓存的状态
    pub status: PortStatus,

    /// xHCI 寄存器访问
    reg: XhciRegistersShared,
}

impl XhciPort {
    /// 创建新的 xHCI 端口
    pub fn new(index: u8, reg_offset: usize, reg: XhciRegistersShared) -> Self {
        Self {
            index,
            reg_offset,
            status: PortStatus {
                connected: false,
                enabled: false,
                suspended: false,
                over_current: false,
                resetting: false,
                powered: false,
                low_speed: false,
                high_speed: false,
                speed: DeviceSpeed::Full,
                change: PortStatusChange {
                    connection_changed: false,
                    enabled_changed: false,
                    reset_complete: false,
                    suspend_changed: false,
                    over_current_changed: false,
                },
            },
            reg,
        }
    }

    /// 读取端口寄存器（PORTSC）
    #[inline]
    fn read_portsc(&self) -> u32 {
        // TODO: 需要根据 xhci crate 的实际 API 调整
        // 暂时返回 0，实际实现需要读取 PORTSC 寄存器
        0
    }

    /// 写入端口寄存器（PORTSC）
    #[inline]
    fn write_portsc(&self, _value: u32) {
        // TODO: 需要根据 xhci crate 的实际 API 调整
        // 暂时空实现，实际实现需要写入 PORTSC 寄存器
    }

    /// 刷新端口状态
    pub fn refresh_status(&mut self) {
        let portsc = self.read_portsc();

        // 读取状态位（参照 xHCI 规范 5.4.8）
        self.status.connected = (portsc & 0x01) != 0; // CCS
        self.status.enabled = (portsc & 0x02) != 0; // PED
        self.status.suspended = (portsc & 0x80) != 0; // PLS (check for U3)

        // 读取变化位
        self.status.change.connection_changed = (portsc & (1 << 17)) != 0; // CSC
        self.status.change.enabled_changed = (portsc & (1 << 18)) != 0; // PEC
        self.status.change.reset_complete = (portsc & (1 << 21)) != 0; // PRC

        // 读取速度（bits 10:13）
        let speed = (portsc >> 10) & 0x0F;
        self.status.speed = match speed {
            1 => DeviceSpeed::Full,
            2 => DeviceSpeed::Low,
            3 => DeviceSpeed::High,
            4 => DeviceSpeed::SuperSpeed,
            5 => DeviceSpeed::SuperSpeedPlus,
            _ => DeviceSpeed::Full,
        };

        self.status.high_speed = matches!(self.status.speed, DeviceSpeed::High);
        self.status.low_speed = matches!(self.status.speed, DeviceSpeed::Low);
    }
}
