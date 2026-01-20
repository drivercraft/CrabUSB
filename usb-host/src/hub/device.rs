//! Hub 设备
//!
//! 表示一个 Hub 设备（Root Hub 或 External Hub），管理端口状态和设备枚举。

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::time::Duration;

use usb_if::{
    descriptor::{Class, EndpointType},
    host::{
        ControlSetup, USBError,
        hub::{DeviceSpeed, HubDescriptor, PortFeature, PortStatus, PortStatusChange},
    },
    transfer::{Recipient, Request, RequestType},
};

use super::event::HubId;
use crate::{Device, DeviceInfo, backend::DeviceId};

// Hub 枚举常量 (参照 Linux 内核)

/// 防抖动超时 (2秒)
const HUB_DEBOUNCE_TIMEOUT: u64 = 2000;

/// 防抖动检查间隔 (25ms)
const HUB_DEBOUNCE_STEP: u64 = 25;

/// 防抖动稳定时间 (100ms)
const HUB_DEBOUNCE_STABLE: u64 = 100;

/// 端口初始化重试次数
const PORT_INIT_TRIES: u8 = 4;

/// 获取描述符重试次数
const GET_DESCRIPTOR_TRIES: u8 = 2;

/// SET_ADDRESS 等待时间 (10ms)
const SET_ADDRESS_SETTLING_TIME: u64 = 10;

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

    pub descriptor: HubDescriptor,
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
    pub async fn new(
        parent_hub: Option<HubId>,
        depth: u8,
        dev: Device,
        config: u8,
        interface: u8,
    ) -> Result<Self, USBError> {
        Ok(Self {
            config,
            interface,
            data: Box::new(Inner {
                state: HubState::Uninitialized,
                num_ports: 0,
                ports: vec![],
                parent_hub,
                depth,
                dev,
                descriptor: unsafe { core::mem::zeroed() },
            }),
        })
    }

    pub fn id(&self) -> HubId {
        self.data.as_ref() as *const Inner as usize as HubId
    }

    pub fn is_superspeed(&self) -> bool {
        self.data.dev.descriptor().protocol == 3
    }

    pub async fn init(&mut self) -> Result<(), USBError> {
        // 第二阶段：获取 Hub 描述符（带重试）
        let descriptor = self.get_hub_descriptor().await?;
        self.data.descriptor = descriptor;
        if self.hub_descriptor().bNbrPorts == 0 {
            return Err(USBError::Other(anyhow!("Hub has zero ports")));
        }
        self.data.num_ports = self.hub_descriptor().bNbrPorts;

        // 第三阶段：初始化端口状态（参考 Linux hub_activate）
        // 初始化所有端口为 Disconnected 状态
        self.data.ports = (1..=self.data.num_ports).map(Port::new).collect();

        self.data.dev.claim_interface(self.interface, 0).await?;

        // 标记 Hub 为运行状态
        self.data.state = HubState::Running;
        debug!("Hub initialized with {} ports", self.data.num_ports);
        Ok(())
    }

    fn hub_descriptor(&self) -> &HubDescriptor {
        &self.data.descriptor
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

    // ========== 端口状态获取方法 ==========

    /// 获取端口状态 (参照 Linux usb_hub_port_status)
    ///
    /// 返回: (端口状态, 状态变化标志)
    async fn get_port_status(
        &mut self,
        port_index: u8,
    ) -> Result<(PortStatus, PortStatusChange), USBError> {
        let mut buffer = [0u8; 4]; // wPortStatus (2字节) + wPortChange (2字节)

        self.data
            .dev
            .ep_ctrl()
            .control_in(
                ControlSetup {
                    request_type: RequestType::Class,
                    recipient: Recipient::Other, // Port
                    request: Request::GetStatus,
                    value: 0,
                    index: port_index as u16,
                },
                &mut buffer,
            )
            .await?;

        // 解析端口状态和变化
        let status_raw = u16::from_le_bytes([buffer[0], buffer[1]]);
        let change_raw = u16::from_le_bytes([buffer[2], buffer[3]]);

        Ok((
            self.parse_port_status(status_raw),
            self.parse_port_change(change_raw),
        ))
    }

    /// 解析端口状态原始数据
    fn parse_port_status(&self, raw: u16) -> PortStatus {
        PortStatus {
            connected: (raw & 0x0001) != 0,
            enabled: (raw & 0x0002) != 0,
            suspended: (raw & 0x0004) != 0,
            over_current: (raw & 0x0008) != 0,
            resetting: (raw & 0x0010) != 0,
            powered: (raw & 0x0100) != 0,
            low_speed: (raw & 0x0200) != 0,
            high_speed: (raw & 0x0400) != 0,
            speed: if (raw & 0x0200) != 0 {
                DeviceSpeed::Low
            } else if (raw & 0x0400) != 0 {
                DeviceSpeed::High
            } else if (raw & 0x0800) != 0 {
                DeviceSpeed::SuperSpeed
            } else {
                DeviceSpeed::Full
            },
            change: PortStatusChange {
                connection_changed: false,
                enabled_changed: false,
                reset_complete: false,
                suspend_changed: false,
                over_current_changed: false,
            },
        }
    }

    /// 解析端口状态变化标志
    fn parse_port_change(&self, raw: u16) -> PortStatusChange {
        PortStatusChange {
            connection_changed: (raw & 0x0001) != 0,
            enabled_changed: (raw & 0x0002) != 0,
            suspend_changed: (raw & 0x0004) != 0,
            over_current_changed: (raw & 0x0008) != 0,
            reset_complete: (raw & 0x0010) != 0,
        }
    }

    /// 设置端口特性
    async fn set_port_feature(
        &mut self,
        port_index: u8,
        feature: PortFeature,
    ) -> Result<(), USBError> {
        self.data
            .dev
            .ep_ctrl()
            .control_out(
                ControlSetup {
                    request_type: RequestType::Class,
                    recipient: Recipient::Other,
                    request: Request::SetFeature,
                    value: feature as u16,
                    index: port_index as u16,
                },
                &[],
            )
            .await
            .map_err(USBError::from)?;
        Ok(())
    }

    /// 清除端口特性
    async fn clear_port_feature(
        &mut self,
        port_index: u8,
        feature: PortFeature,
    ) -> Result<(), USBError> {
        self.data
            .dev
            .ep_ctrl()
            .control_out(
                ControlSetup {
                    request_type: RequestType::Class,
                    recipient: Recipient::Other,
                    request: Request::ClearFeature,
                    value: feature as u16,
                    index: port_index as u16,
                },
                &[],
            )
            .await
            .map_err(USBError::from)?;
        Ok(())
    }

    // ========== 防抖动机制 ==========

    /// 防抖动检测 (参照 Linux hub_port_debounce_be_stable)
    ///
    /// 确保端口连接状态稳定，避免抖动导致误判。
    ///
    /// # 参数
    /// - `port_index`: 端口号（1-based）
    /// - `must_be_connected`: 期望的连接状态
    ///
    /// # 返回
    /// 稳定后的端口状态
    async fn debounce_port(
        &mut self,
        port_index: u8,
        must_be_connected: bool,
    ) -> Result<PortStatus, USBError> {
        let mut stable_count = 0u8;
        let required_stable = (HUB_DEBOUNCE_STABLE / HUB_DEBOUNCE_STEP) as u8;
        let max_attempts = (HUB_DEBOUNCE_TIMEOUT / HUB_DEBOUNCE_STEP) as u8;

        info!(
            "Starting debounce on port {} (expected_connected: {})",
            port_index, must_be_connected
        );

        for attempt in 0..max_attempts {
            // 等待检查间隔（25ms）
            crate::osal::kernel::delay(core::time::Duration::from_millis(HUB_DEBOUNCE_STEP));

            // 获取当前状态
            let (status, _change) = self.get_port_status(port_index).await?;

            // 验证连接状态是否符合期望
            if status.connected == must_be_connected {
                stable_count = stable_count.saturating_add(1);
                debug!(
                    "Port {} debounce stable: {}/{} (attempt {})",
                    port_index, stable_count, required_stable, attempt
                );

                if stable_count >= required_stable {
                    info!(
                        "Port {} debounce stable (connected: {})",
                        port_index, status.connected
                    );
                    return Ok(status);
                }
            } else {
                // 状态不稳定，重置计数
                stable_count = 0;
                debug!(
                    "Port {} debounce unstable, current_connected: {}, expected: {}",
                    port_index, status.connected, must_be_connected
                );
            }
        }

        // 超时
        warn!(
            "Port {} debounce timeout after {} attempts ({}ms)",
            port_index, max_attempts, HUB_DEBOUNCE_TIMEOUT
        );
        Err(USBError::Timeout)
    }

    // ========== 设备枚举核心方法 ==========

    /// 端口复位 (参照 Linux hub_port_reset)
    ///
    /// 复位端口并等待复位完成。
    ///
    /// # 参数
    /// - `port_index`: 端口号（1-based）
    /// - `status`: 当前端口状态
    async fn reset_port(&mut self, port_index: u8, status: &PortStatus) -> Result<(), USBError> {
        info!("Resetting port {}", port_index);

        // 发送复位请求
        self.set_port_feature(port_index, PortFeature::Reset)
            .await?;

        // 确定复位时间（低速设备需要长复位）
        let reset_time = if status.low_speed {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(50)
        };

        // 等待复位完成
        crate::osal::kernel::delay(reset_time);

        // 等待复位完成标志（最多等待 100ms）
        for _retry in 0..10 {
            let (_status, change) = self.get_port_status(port_index).await?;

            if change.reset_complete {
                // 清除复位完成标志
                self.clear_port_feature(port_index, PortFeature::CReset)
                    .await?;
                info!("Port {} reset complete", port_index);
                return Ok(());
            }

            crate::osal::kernel::delay(Duration::from_millis(10));
        }

        warn!("Port {} reset timeout", port_index);
        Err(USBError::Timeout)
    }

    pub fn probe_devices(&mut self) -> Result<Vec<DeviceInfo>, USBError> {
        // ========================================================================
        // External Hub 设备枚举架构说明
        // ========================================================================
        //
        // **当前限制**：
        // External Hub 端口上的设备枚举需要底层 USB 控制器（xHCI）支持。
        // HubDevice 只能通过 Hub 特定请求（GetPortStatus、SetPortFeature）
        // 来管理端口状态，但无法直接完成设备枚举流程。
        //
        // **设备枚举需要的操作**：
        // 1. 端口复位 ✅ HubDevice 可以通过 SetPortFeature(Reset) 实现
        // 2. 地址分配 ❌ 需要控制器 Enable Slot 命令
        // 3. 获取描述符 ❌ 需要通过控制端点与设备通信
        // 4. 配置设置 ❌ 需要控制器端点管理
        //
        // **正确的架构**：
        // - Hub 层：端口状态监控、复位、防抖动检测
        // - Controller 层：设备枚举、地址分配、端点管理
        //
        // **实现方案**：
        // 方案 1：由 Host/Controller 层监听 Hub 端口状态变化，
        //         触发设备枚举（类似 Linux 的 hub_event）
        //
        // 方案 2：扩展 HubDevice 接口，提供端口状态回调，
        //         由 Host 层完成枚举
        //
        // 方案 3：为每个 Hub 端口创建虚拟 xHCI 端口映射，
        //         复用现有枚举流程（复杂度高）
        //
        // **当前状态**：
        // - Root Hub 枚举：xHCI::_probe_devices() 已实现
        // - External Hub 枚举：需要架构扩展
        //
        // **相关代码**：
        // - usb-host/src/backend/xhci/host.rs: _probe_devices()
        // - usb-host/src/host.rs: probe_handle_hub()
        // ========================================================================

        // 暂时返回空列表
        // 完整实现需要架构调整或端口事件系统支持
        Ok(vec![])
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
