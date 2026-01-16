//! USB Hub 设备抽象
//!
//! 定义 Root Hub 和 External Hub 的共性接口。
//!
//! # 架构设计
//!
//! ```text
//!     Hub (基础 trait)
//!      ├─ 端口管理
//!      ├─ 端口状态查询
//!      └─ 描述符访问
//!           │
//!           ├──────────────┬──────────────┐
//!           ▼              ▼              ▼
//!     RootHub      ExternalHub    TransactionHub
//!     (寄存器访问)   (USB 设备)     (通过传输)
//! ```
//!
//! # Linux 对应关系
//!
//! | CrabUSB | Linux |
//! |---------|-------|
//! | `Hub` | `struct usb_hub` |
//! | `PortOps` | `hub_port_*` functions |
//! | `RootHub` | `struct usb_hcd` + rh_dev |
//! | `ExternalHub` | `drivers/usb/core/hub.c` |

use alloc::{boxed::Box, vec::Vec};
use futures::future::LocalBoxFuture;

use crate::{
    descriptor::{DeviceDescriptor, ConfigurationDescriptor},
    err::USBError,
    transfer::ControlSetup,
};

use super::{DeviceInfo, Device};

// ============================================================================
// 基础 Hub 接口
// ============================================================================

/// Hub 设备基础接口
///
/// 定义 Root Hub 和 External Hub 的共性操作。
/// 参照 USB 2.0 规范第 11 章和 Linux `struct usb_hub`。
pub trait Hub: Send + 'static {
    /// 获取 Hub 描述符
    ///
    /// 返回 Hub 的 Class 描述符（USB 2.0 规范 11.23.2.1）
    fn hub_descriptor(&self) -> LocalBoxFuture<'_, Result<HubDescriptor, USBError>>;

    /// 获取端口数量
    ///
    /// Root Hub: 从寄存器读取
    /// External Hub: 从 Hub 描述符读取
    fn num_ports(&self) -> u8;

    /// 获取指定端口的操作接口
    ///
    /// # Safety
    /// 调用者必须确保 port_index < num_ports()
    fn port(&mut self, port_index: u8) -> Result<Box<dyn HubPortOps>, USBError>;

    /// 获取所有端口的当前状态
    ///
    /// 用于设备枚举和状态监控
    fn port_status_all(&mut self) -> LocalBoxFuture<'_, Result<Vec<PortStatus>, USBError>>;

    /// 获取 Hub 特性
    fn hub_characteristics(&self) -> HubCharacteristics;

    /// 电源控制
    ///
    /// - Root Hub: 通常所有端口共享电源，无法单独控制
    /// - External Hub: 可以根据描述符控制每个端口的电源
    fn power_switching_mode(&self) -> PowerSwitchingMode;

    /// 处理 Hub 事件
    ///
    /// - Root Hub: 由 Host Controller 中断触发
    /// - External Hub: 通过状态变化端点接收
    ///
    /// # Safety
    /// 必须在适当的上下文中调用（中断或任务上下文）
    unsafe fn handle_event(&mut self) -> LocalBoxFuture<'_, Result<(), USBError>>;
}

/// Hub 端口操作接口
///
/// 定义单个端口的操作，参照 USB 2.0 规范 11.24。
pub trait HubPortOps: Send + Sync {
    /// 获取端口号
    fn port_number(&self) -> u8;

    /// 读取端口状态
    ///
    /// 返回端口的当前状态，参照 USB 2.0 规范表 11-21。
    ///
    /// # Safety
    /// 调用者应确保在中断禁用或持有适当锁的情况下调用
    unsafe fn read_status(&self) -> Result<PortStatus, USBError>;

    /// 端口复位
    ///
    /// 参照 USB 2.0 规范 11.5.1.5。
    /// 复位后，端口会进入 Enabled 状态。
    ///
    /// # 特殊情况
    /// - Root Hub: 直接操作寄存器
    /// - External Hub: 发送 SetPortFeature(PORT_RESET) 请求
    async fn reset(&mut self) -> Result<(), USBError>;

    /// 启用/禁用端口
    ///
    /// 参照 USB 2.0 规范 11.24.2.7。
    async fn set_enable(&mut self, enable: bool) -> Result<(), USBError>;

    /// 电源控制
    ///
    /// 控制端口的电源供应。
    ///
    /// - Root Hub: 通常无法单独控制（所有端口共享）
    /// - External Hub: 根据 Hub 特性可能支持逐端口控制
    async fn set_power(&mut self, power: bool) -> Result<(), USBError>;

    /// 挂起/恢复
    ///
    /// 参照 USB 2.0 规范 11.5.1.8。
    async fn set_suspend(&mut self, suspend: bool) -> Result<(), USBError>;

    /// 清除端口状态变化标志
    ///
    /// 端口状态变化（连接、复位等）需要在处理后清除。
    ///
    /// # Safety
    /// 必须在处理完变化后调用，避免丢失事件
    unsafe fn clear_status_change(&mut self) -> Result<(), USBError>;

    /// 检测连接的设备速度
    ///
    /// 返回端口连接设备的速度。
    /// 如果没有设备连接，返回 None。
    ///
    /// - Root Hub: 从端口寄存器读取
    /// - External Hub: 从状态读取
    fn device_speed(&self) -> Option<DeviceSpeed>;

    /// 检查是否是高速端口
    ///
    /// 用于确定是否需要 Transaction Translator (TT)。
    fn is_high_speed(&self) -> bool;
}

// ============================================================================
// Root Hub 特定接口
// ============================================================================

/// Root Hub 接口
///
/// Root Hub 是集成在 Host Controller 内的虚拟 Hub。
/// 它直接访问控制器寄存器，不需要通过 USB 传输。
///
/// 参照 Linux `struct usb_hcd` 的根集线器实现。
pub trait RootHub: Hub {
    /// 获取 Host Controller 引用
    ///
    /// 用于访问底层硬件寄存器。
    fn host_controller(&self) -> &dyn HostControllerOps;

    /// 可变访问 Host Controller
    fn host_controller_mut(&mut self) -> &mut dyn HostControllerOps;

    /// 等待控制器运行完成
    ///
    /// Root Hub 初始化后需要等待 HCD 进入运行状态。
    async fn wait_for_running(&mut self) -> Result<(), USBError>;

    /// 重置所有端口
    ///
    /// Host Controller 初始化时调用。
    fn reset_all_ports(&mut self) -> Result<(), USBError>;

    /// 启用/禁用中断
    ///
    /// Root Hub 通常直接由 HCD 中断驱动。
    fn enable_irq(&mut self) -> Result<(), USBError>;
    fn disable_irq(&mut self) -> Result<(), USBError>;
}

/// Host Controller 操作接口
///
/// Root Hub 用于访问底层硬件的接口。
pub trait HostControllerOps: Send + Sync {
    /// 读取寄存器
    ///
    /// # Safety
    /// 调用者必须确保寄存器有效
    unsafe fn read_reg(&self, offset: usize, width: RegWidth) -> u64;

    /// 写入寄存器
    ///
    /// # Safety
    /// 调用者必须确保寄存器有效且值合法
    unsafe fn write_reg(&self, offset: usize, width: RegWidth, value: u64);

    /// 内存屏障
    fn barrier(&self, barrier: MemoryBarrierType);

    /// 获取 MMIO 基地址
    fn mmio_base(&self) -> usize;

    /// DMA 映射
    unsafe fn dma_map(&self, virt_addr: usize, size: usize) -> Result<usize, USBError>;

    /// DMA 解映射
    unsafe fn dma_unmap(&self, phys_addr: usize, size: usize);
}

/// 寄存器宽度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegWidth {
    U8,
    U16,
    U32,
    U64,
}

/// 内存屏障类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryBarrierType {
    Read,
    Write,
    Full,
}

// ============================================================================
// External Hub 特定接口
// ============================================================================

/// External Hub 接口
///
/// External Hub 是通过 USB 总线连接的独立 Hub 设备。
/// 所有操作都通过 USB Control 传输完成。
///
/// 参照 Linux `drivers/usb/core/hub.c`。
pub trait ExternalHub: Hub + Device {
    /// 获取 Hub 设备地址
    fn device_address(&self) -> u8;

    /// 获取状态变化端点
    ///
    /// Hub 使用中断端点报告端口状态变化。
    fn status_change_endpoint(&mut self) -> Result<Box<dyn EndpointInterruptIn>, USBError>;

    /// 发送 Hub 类请求
    ///
    /// 参照 USB 2.0 规范 11.24 (Hub Class Requests)。
    fn hub_control(
        &mut self,
        request: HubRequest,
        value: u16,
        index: u16,
        data: &mut [u8],
    ) -> LocalBoxFuture<'_, Result<usize, USBError>>;

    /// 获取 Transaction Translator (TT) 信息
    ///
    /// 用于高速 Hub 与低速/全速设备通信。
    fn tt_info(&self) -> Option<TtInfo>;

    /// 检查是否需要 TT
    ///
    /// 如果 Hub 是高速的，且连接了低速/全速设备，需要 TT。
    fn needs_tt(&self) -> bool;
}

/// Hub 类请求
///
/// 参照 USB 2.0 规范表 11-15。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubRequest {
    GetHubDescriptor,
    GetHubStatus,
    SetHubFeature,
    ClearHubFeature,
    GetPortStatus,
    SetPortFeature,
    ClearPortFeature,
    GetHubDescriptor16, // USB 3.0+
}

/// Transaction Translator 信息
///
/// 用于高速 Hub 与低速/全速设备的通信。
#[derive(Debug, Clone, Copy)]
pub struct TtInfo {
    /// TT 思考时间（单位：2 微秒）
    pub think_time: u8,

    /// 是否有多个 TT
    pub multi_tt: bool,

    /// TT 端口数量
    pub num_ports: u8,
}

// ============================================================================
// 共享数据结构
// ============================================================================

/// Hub 描述符
///
/// 参照 USB 2.0 规范 11.23.2.1。
#[derive(Debug, Clone)]
pub struct HubDescriptor {
    /// 端口数量
    pub num_ports: u8,

    /// Hub 特性
    pub characteristics: HubCharacteristics,

    /// 电源开通到电源良好的时间（单位：2ms）
    pub power_good_time: u8,

    /// Hub 控制器电流（单位：mA）
    pub hub_current: u8,
}

/// Hub 特性
///
/// 参照 USB 2.0 规范图 11-16。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HubCharacteristics {
    /// 电源切换模式
    pub power_switching: PowerSwitchingMode,

    /// 复合设备
    pub compound_device: bool,

    /// 过流保护模式
    pub over_current_mode: OverCurrentMode,

    /// 端口指示灯支持
    pub port_indicators: bool,
}

/// 电源切换模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSwitchingMode {
    /// 所有端口同时供电
    Ganged,

    /// 每个端口独立控制
    Individual,

    /// 无电源控制（总是供电）
    AlwaysPower,
}

/// 过流保护模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverCurrentMode {
    /// 全局过流保护
    Global,

    /// 每个端口独立保护
    Individual,
}

/// 端口状态
///
/// 参照 USB 2.0 规范表 11-21。
#[derive(Debug, Clone, Copy)]
pub struct PortStatus {
    /// 当前连接状态
    pub connected: bool,

    /// 端口已启用
    pub enabled: bool,

    /// 已挂起
    pub suspended: bool,

    /// 过流检测
    pub over_current: bool,

    /// 复位中
    pub resetting: bool,

    /// 电源已开启
    pub powered: bool,

    /// 低速设备连接
    pub low_speed: bool,

    /// 高速设备连接
    pub high_speed: bool,

    /// 端口速度
    pub speed: DeviceSpeed,

    /// 端口状态变化标志
    pub change: PortStatusChange,
}

/// 端口状态变化标志
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortStatusChange {
    /// 连接状态变化
    pub connection_changed: bool,

    /// 启用状态变化
    pub enabled_changed: bool,

    /// 复位完成
    pub reset_complete: bool,

    /// 挂起状态变化
    pub suspend_changed: bool,

    /// 过流状态变化
    pub over_current_changed: bool,
}

/// USB 设备速度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeviceSpeed {
    Low = 0,
    Full = 1,
    High = 2,
    Wireless = 3,
    SuperSpeed = 4,
    SuperSpeedPlus = 5,
}

impl From<u8> for DeviceSpeed {
    fn from(value: u8) -> Self {
        match value {
            0 => DeviceSpeed::Low,
            1 => DeviceSpeed::Full,
            2 => DeviceSpeed::High,
            3 => DeviceSpeed::Wireless,
            4 => DeviceSpeed::SuperSpeed,
            5 => DeviceSpeed::SuperSpeedPlus,
            _ => DeviceSpeed::Full,
        }
    }
}

// ============================================================================
// 端点接口（复用）
// ============================================================================

/// 中断 IN 端点
///
/// 用于 External Hub 的状态变化报告。
pub trait EndpointInterruptIn: Send + 'static {
    fn submit<'a>(&mut self, data: &'a mut [u8])
        -> LocalBoxFuture<'a, Result<usize, USBError>>;
}

// ============================================================================
// 辅助函数
// ============================================================================

impl HubCharacteristics {
    /// 从描述符原始数据解析
    ///
    /// 参照 USB 2.0 规范图 11-16。
    pub fn from_descriptor(value: u16) -> Self {
        let power_switching = match (value & 0x03) {
            0x01 => PowerSwitchingMode::Ganged,
            0x02 => PowerSwitchingMode::Individual,
            _ => PowerSwitchingMode::AlwaysPower,
        };

        let compound_device = (value & 0x04) != 0;
        let over_current_mode = if (value & 0x08) != 0 {
            OverCurrentMode::Individual
        } else {
            OverCurrentMode::Global
        };
        let port_indicators = (value & 0x10) != 0;

        Self {
            power_switching,
            compound_device,
            over_current_mode,
            port_indicators,
        }
    }

    /// 转换为描述符原始数据
    pub fn to_descriptor(&self) -> u16 {
        let mut value = 0u16;

        value |= match self.power_switching {
            PowerSwitchingMode::Ganged => 0x01,
            PowerSwitchingMode::Individual => 0x02,
            PowerSwitchingMode::AlwaysPower => 0x00,
        };

        if self.compound_device {
            value |= 0x04;
        }

        if matches!(self.over_current_mode, OverCurrentMode::Individual) {
            value |= 0x08;
        }

        if self.port_indicators {
            value |= 0x10;
        }

        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hub_characteristics_roundtrip() {
        let original = HubCharacteristics {
            power_switching: PowerSwitchingMode::Individual,
            compound_device: true,
            over_current_mode: OverCurrentMode::Global,
            port_indicators: true,
        };

        let descriptor = original.to_descriptor();
        let decoded = HubCharacteristics::from_descriptor(descriptor);

        assert_eq!(original.power_switching, decoded.power_switching);
        assert_eq!(original.compound_device, decoded.compound_device);
        assert_eq!(original.over_current_mode, decoded.over_current_mode);
        assert_eq!(original.port_indicators, decoded.port_indicators);
    }
}
