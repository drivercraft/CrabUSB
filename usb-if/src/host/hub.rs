//! USB Hub 设备抽象

use alloc::{ vec::Vec};

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
