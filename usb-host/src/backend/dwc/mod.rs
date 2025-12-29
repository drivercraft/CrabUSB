//! DWC3 (DesignWare USB3 Controller) 驱动
//!
//! DWC3 是一个 USB3 DRD (Dual Role Device) 控制器，支持 Host 和 Device 模式。
//! 本模块实现 Host 模式驱动，基于 xHCI 规范。

use alloc::vec::Vec;

use crate::{Mmio, Xhci, backend::ty::HostOp, err::Result};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

pub mod phy;
mod reg;

pub use phy::{UsbDpMode, UsbDpPhy, UsbDpPhyConfig};
use reg::Dwc3Regs;

/// DWC3 控制器
///
/// DWC3 实际上是 xHCI 主机控制器的封装。在 Host 模式下，
/// DWC3 的 xHCI 寄存器区域 (0x0000 - 0x7fff) 包含标准 xHCI 寄存器，
/// 全局寄存器区域 (0xc100 - 0xcfff) 包含 DWC3 特定配置。
pub struct Dwc {
    xhci: Xhci,
    phy: UsbDpPhy,
    dwc_regs: Dwc3Regs,
}

impl Dwc {
    /// 创建新的 DWC3 控制器实例
    ///
    /// # 参数
    ///
    /// * `mmio` - MMIO 基址
    /// * `dma_mask` - DMA 掩码
    ///
    /// # 初始化流程
    ///
    /// 1. 验证 SNPSID 寄存器
    /// 2. 设置为 HOST 模式
    /// 3. 初始化 xHCI 主机控制器
    pub fn new(ctrl: Mmio, phy: Mmio, dma_mask: usize) -> Result<Self> {
        let mmio_base = ctrl.as_ptr() as usize;
        let phy = UsbDpPhy::new(
            UsbDpPhyConfig {
                mode: UsbDpMode::Usb,
                ..Default::default()
            },
            phy,
        );

        let xhci = Xhci::new(ctrl, dma_mask)?;

        let dwc_regs = unsafe { Dwc3Regs::new(mmio_base) };

        Ok(Self {
            xhci,
            dwc_regs,
            phy,
        })
    }
}

impl HostOp for Dwc {
    type DeviceInfo = DeviceInfo;
    type EventHandler = EventHandler;

    /// 初始化 DWC3 控制器
    async fn init(&mut self) -> Result {
        log::info!("DWC3: Starting controller initialization");

        // 2. 配置 PHY
        let phy_config = UsbDpPhyConfig {
            mode: UsbDpMode::Usb,
            ..Default::default()
        };
        self.phy.config = phy_config;
        self.phy.init();

        // 步骤 1: 验证 SNPSID
        self.dwc_regs.verify_snpsid();

        // 步骤 2: 配置 GCTL 寄存器（必须在设置模式之前）
        self.dwc_regs.setup_gctl();

        // 步骤 3: 设置 HOST 模式
        self.dwc_regs.setup_host_mode();

        // 步骤 4: 配置 PHY（包括复位序列）
        self.dwc_regs.setup_phy();

        // 步骤 5: 核心软复位 (HOST 模式下由 xHCI 处理)
        log::debug!("DWC3 soft reset skipped (HOST mode, handled by xHCI)");

        // 步骤 6: 初始化 xHCI 主机控制器
        log::info!("DWC3: Initializing xHCI host controller");
        self.xhci.init().await?;

        log::info!("✓ DWC3 controller initialized successfully");

        Ok(())
    }

    /// 探测 USB 设备
    async fn probe_devices(&mut self) -> Result<Vec<Self::DeviceInfo>> {
        let devices = self.xhci.probe_devices().await?;
        Ok(devices)
    }

    /// 打开 USB 设备
    async fn open_device(
        &mut self,
        dev: &Self::DeviceInfo,
    ) -> Result<<Self::DeviceInfo as super::ty::DeviceInfoOp>::Device> {
        let device = self.xhci.open_device(dev).await?;
        Ok(device)
    }

    /// 创建事件处理器
    fn create_event_handler(&mut self) -> Self::EventHandler {
        self.xhci.create_event_handler()
    }
}
