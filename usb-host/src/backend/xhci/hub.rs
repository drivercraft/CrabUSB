//! xHCI Root Hub 实现
//!
//! 实现 xHCI 控制器的 Root Hub 功能，遵循 xHCI 规范第 4.19 章。

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::time::Duration;
use futures::future::LocalBoxFuture;
use futures::task::AtomicWaker;
use spin::RwLock;

use usb_if::host::{USBError, hub::PortStatusChange};

use crate::backend::xhci::port::XhciPort;
use crate::backend::xhci::reg::XhciRegistersShared;

/// xHCI Root Hub
///
/// Root Hub 是集成在 xHCI 控制器中的虚拟 Hub。
pub struct XhciRootHub {
    /// 寄存器访问
    reg: XhciRegistersShared,

    /// 端口数量（从 HCSPARAMS1 读取）
    num_ports: u8,

    /// 端口数组
    ports: Vec<XhciPort>,

    /// Hub 状态
    state: HubState,

    /// DMA mask
    dma_mask: usize,
}

/// Hub 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubState {
    /// 未初始化
    Uninitialized,

    /// 初始化中
    Initializing,

    /// 运行中
    Running,

    /// 挂起
    Suspended,
}

impl XhciRootHub {
    /// 创建新的 xHCI Root Hub
    pub fn new(reg: XhciRegistersShared, dma_mask: usize) -> Result<Self, USBError> {
        // 读取端口数量（从 HCSPARAMS1 寄存器）
        {
            let regs = reg.read();
            let _hcs_params1 = regs.capability.hcsparams1.read_volatile();
            // TODO: xhci crate 的 API 可能需要调整
            // 暂时使用默认值 4 个端口
        }
        let num_ports = 4u8;

        // 创建端口数组
        let mut ports = Vec::with_capacity(num_ports as usize);
        for i in 0..num_ports {
            // 端口寄存器偏移计算
            // xHCI 规范：端口寄存器从 operational base + 0x400 开始
            // 每个端口占用 0x10 字节（4 个 32 位寄存器）
            let reg_offset = 0x400 + (i as usize * 0x10);
            ports.push(XhciPort::new(i + 1, reg_offset, reg.clone()));
        }

        Ok(Self {
            reg,
            num_ports,
            ports,
            state: HubState::Uninitialized,
            dma_mask,
        })
    }

    /// 获取端口数组（可变）
    pub fn ports_mut(&mut self) -> &mut [XhciPort] {
        &mut self.ports
    }

    /// 刷新所有端口状态
    pub fn refresh_all_ports(&mut self) {
        for port in &mut self.ports {
            port.refresh_status();
        }
    }

    /// 处理端口状态变化事件
    pub fn handle_port_status_change(
        &mut self,
        port_index: u8,
    ) -> Result<PortStatusChange, USBError> {
        if port_index == 0 || port_index > self.num_ports {
            return Err(USBError::InvalidParameter);
        }

        let port = &mut self.ports[(port_index - 1) as usize];
        port.refresh_status();

        Ok(port.status.change)
    }
}

// TODO: 暂时注释掉 Hub trait 实现，等基础设施就绪后再实现
// impl Hub for XhciRootHub {
//     fn hub_descriptor(&self) -> LocalBoxFuture<'_, Result<HubDescriptor, USBError>> {
//         async {
//             // Root Hub 没有 Hub 描述符，返回默认值
//             Ok(HubDescriptor {
//                 num_ports: self.num_ports,
//                 characteristics: HubCharacteristics {
//                     power_switching: PowerSwitchingMode::AlwaysPower,
//                     compound_device: false,
//                     over_current_mode: OverCurrentMode::Global,
//                     port_indicators: false,
//                 },
//                 power_good_time: 0,    // Root Hub 不需要
//                 hub_current: 0,        // Root Hub 不需要
//             })
//         }.boxed()
//     }
//
//     fn num_ports(&self) -> u8 {
//         self.num_ports
//     }
//
//     fn port(&mut self, port_index: u8) -> Result<Box<dyn HubPortOps>, USBError> {
//         if port_index == 0 || port_index > self.num_ports {
//             return Err(USBError::InvalidParameter);
//         }
//         // TODO: 返回 Box::new(self.ports[(port_index - 1) as usize].clone())
//         Err(USBError::NotSupported)
//     }
//
//     fn port_status_all(&mut self) -> LocalBoxFuture<'_, Result<Vec<PortStatus>, USBError>> {
//         async {
//             self.refresh_all_ports();
//             Ok(self.ports.iter().map(|p| p.status).collect())
//         }.boxed()
//     }
//
//     fn hub_characteristics(&self) -> HubCharacteristics {
//         HubCharacteristics {
//             power_switching: PowerSwitchingMode::AlwaysPower,
//             compound_device: false,
//             over_current_mode: OverCurrentMode::Global,
//             port_indicators: false,
//         }
//     }
//
//     fn power_switching_mode(&self) -> PowerSwitchingMode {
//         PowerSwitchingMode::AlwaysPower
//     }
//
//     unsafe fn handle_event(&mut self) -> LocalBoxFuture<'_, Result<(), USBError>> {
//         async {
//             // 刷新所有端口状态
//             self.refresh_all_ports();
//             Ok(())
//         }.boxed()
//     }
// }

// TODO: 暂时注释掉 RootHub trait 实现，等基础设施就绪后再实现
// impl RootHub for XhciRootHub {
//     fn host_controller(&self) -> &dyn HostControllerOps {
//         self
//     }
//
//     fn host_controller_mut(&mut self) -> &mut dyn HostControllerOps {
//         self
//     }
//
//     async fn wait_for_running(&mut self) -> Result<(), USBError> {
//         // 等待 USBSTS.HCHalted == 0 && CNR == 0
//         loop {
//             unsafe {
//                 let regs = self.reg.read();
//                 let usbsts = regs.operational.usbsts.read_volatile();
//                 if !usbsts.hc_halted() && !usbsts.controller_not_ready() {
//                     return Ok(());
//                 }
//             }
//             osal::kernel::delay(Duration::from_millis(10)).await;
//         }
//     }
//
//     fn reset_all_ports(&mut self) -> Result<(), USBError> {
//         // 1. 开启所有端口电源
//         for port in &mut self.ports {
//             port.set_power(true)?;
//         }
//
//         // 2. 等待电源稳定（USB 2.0 规范 11.11）
//         // TODO: 需要异步延迟
//
//         // 3. 复位所有端口
//         // TODO: 需要异步复位
//
//         Ok(())
//     }
//
//     fn enable_irq(&mut self) -> Result<(), USBError> {
//         unsafe {
//             let regs = self.reg.read();
//             regs.operational.usbcmd.update_volatile(|r| {
//                 r.set_interrupter_enable();
//             });
//         }
//         Ok(())
//     }
//
//     fn disable_irq(&mut self) -> Result<(), USBError> {
//         unsafe {
//             let regs = self.reg.read();
//             regs.operational.usbcmd.update_volatile(|r| {
//                 r.clear_interrupter_enable();
//             });
//         }
//         Ok(())
//     }
// }
