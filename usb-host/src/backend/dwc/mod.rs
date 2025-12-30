//! DWC3 (DesignWare USB3 Controller) 驱动
//!
//! DWC3 是一个 USB3 DRD (Dual Role Device) 控制器，支持 Host 和 Device 模式。
//! 本模块实现 Host 模式驱动，基于 xHCI 规范。

use alloc::vec::Vec;

use crate::{Mmio, Xhci, backend::ty::HostOp, err::Result};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

pub mod cru;
pub mod grf;
pub mod phy;
pub mod usb2phy;
mod reg;

pub use cru::Cru;
pub use phy::{UsbDpMode, UsbDpPhy, UsbDpPhyConfig};
pub use usb2phy::Usb2Phy;
use reg::Dwc3Regs;

/// DWC3 控制器
///
/// DWC3 实际上是 xHCI 主机控制器的封装。在 Host 模式下，
/// DWC3 的 xHCI 寄存器区域 (0x0000 - 0x7fff) 包含标准 xHCI 寄存器，
/// 全局寄存器区域 (0xc100 - 0xcfff) 包含 DWC3 特定配置。
pub struct Dwc {
    xhci: Xhci,
    phy: UsbDpPhy,
    usb2_phy: Usb2Phy,
    cru: Cru,
    dwc_regs: Dwc3Regs,
}

impl Dwc {
    /// 创建新的 DWC3 控制器实例
    ///
    /// # 参数
    ///
    /// * `ctrl` - DWC3 控制器 MMIO 基址
    /// * `phy` - USBDP PHY MMIO 基址
    /// * `usb2_phy` - USB2 PHY MMIO 基址（用于启动 480MHz 时钟）
    /// * `usb_grf` - USB GRF 基址
    /// * `dp_grf` - USBDP PHY GRF 基址
    /// * `usb2phy_grf` - USB2PHY GRF 基址
    /// * `cru` - CRU (时钟和复位单元) MMIO 基址
    /// * `dma_mask` - DMA 掩码
    ///
    /// # 初始化流程
    ///
    /// 1. 验证 SNPSID 寄存器
    /// 2. 设置为 HOST 模式
    /// 3. 初始化 xHCI 主机控制器
    pub fn new(
        ctrl: Mmio,
        phy: Mmio,
        usb2_phy: Mmio,
        usb_grf: Mmio,
        dp_grf: Mmio,
        usb2phy_grf: Mmio,
        cru: Mmio,
        dma_mask: usize,
    ) -> Result<Self> {
        let mmio_base = ctrl.as_ptr() as usize;
        let cru = unsafe { Cru::new(cru) };

        // RK3588 有两个 USB3 控制器：
        // - USB3OTG0 (port 0): 通常对应控制器 ID 0
        // - USB3OTG1 (port 1): 通常对应控制器 ID 1
        //
        // 从设备树和寄存器地址判断：
        // - USB3OTG0 基址: 0xfc000000
        // - USB3OTG1 基址: 0xfc400000
        //
        // 根据 ctrl 基址确定 PHY ID
        let phy_id = if (ctrl.as_ptr() as usize) >= 0xfc400000 {
            1 // USB3OTG1
        } else {
            0 // USB3OTG0
        };

        let phy = UsbDpPhy::new(
            UsbDpPhyConfig {
                id: phy_id, // 根据控制器基址确定 PHY ID
                mode: UsbDpMode::Usb,
                ..Default::default()
            },
            phy,
            usb_grf,
            dp_grf,
            usb2phy_grf,
            cru,
        );

        // 创建 USB2 PHY（用于提供 480MHz 时钟给 DWC3）
        let usb2_phy = unsafe { Usb2Phy::new(usb2_phy) };

        let xhci = Xhci::new(ctrl, dma_mask)?;

        let dwc_regs = unsafe { Dwc3Regs::new(mmio_base) };

        Ok(Self {
            xhci,
            dwc_regs,
            phy,
            usb2_phy,
            cru,
        })
    }
}

impl HostOp for Dwc {
    type DeviceInfo = DeviceInfo;
    type EventHandler = EventHandler;

    /// 初始化 DWC3 控制器
    ///
    /// ## 初始化顺序说明
    ///
    /// 在 HOST 模式下，必须按照以下顺序初始化：
    /// 1. USBDP PHY 硬件初始化（时钟、复位、PLL）
    /// 2. DWC3 全局配置（GCTL、HOST 模式）
    /// 3. **xHCI 主机控制器初始化**（执行 HCRST 复位）
    /// 4. DWC3 PHY 配置寄存器（GUSB3PIPECTL、GUSB2PHYCFG）
    ///
    /// **关键点**：DWC3 PHY 配置寄存器必须在 xHCI 执行 HCRST **之后**才能访问，
    /// 因为 HCRST 会复位并使能 host block 的 PHY 接口。
    async fn init(&mut self) -> Result {
        log::info!("DWC3: Starting controller initialization");

        // 步骤 0: 使能 DWC3 控制器时钟（必须在访问寄存器之前）
        log::info!("DWC3: Enabling DWC3 controller clocks");
        self.cru.enable_dwc3_controller_clocks();

        // 步骤 0.5: 解除 DWC3 控制器复位（必须在访问寄存器之前）
        log::info!("DWC3: Deasserting DWC3 controller reset");
        self.cru.deassert_dwc3_reset();

        // 等待复位解除生效
        log::debug!("DWC3: Waiting 10ms after reset deassert");
        self.dwc_regs.delay_ms(10);

        // 步骤 1: 初始化 USB2 PHY（启动 480MHz 时钟）
        //
        // ⚠️ 关键步骤！USB2 PHY 输出 480MHz 时钟给 DWC3 控制器。
        // 这个时钟是 DWC3 PHY 接口工作的必要条件。
        // 即使只使用 USB3，也需要 USB2 PHY 的时钟。
        log::info!("DWC3: Initializing USB2 PHY (for 480MHz clock)");
        self.usb2_phy.init_minimal();

        // 步骤 2: 配置 USBDP PHY 硬件（时钟、复位、PLL 等）
        let phy_config = UsbDpPhyConfig {
            mode: UsbDpMode::Usb,
            ..Default::default()
        };
        self.phy.config = phy_config;
        self.phy.init()?;

        // 步骤 3: 验证 SNPSID
        self.dwc_regs.verify_snpsid();

        // 步骤 4: 配置 GCTL 寄存器（必须在设置模式之前）
        self.dwc_regs.setup_gctl();

        // 步骤 5: 设置 HOST 模式
        self.dwc_regs.setup_host_mode();

        // 步骤 6: 初始化 xHCI 主机控制器（执行 HCRST 复位）
        //
        // ⚠️ 关键步骤！xHCI 的 chip_hardware_reset() 会执行 HCRST，
        // 这会复位并使能 DWC3 host block 的 PHY 接口。
        // 只有在 HCRST 之后，DWC3 PHY 配置寄存器才能访问。
        log::info!("DWC3: Initializing xHCI host controller (will execute HCRST)");
        self.xhci.init().await?;

        // 步骤 5.5: 检查 USBDP PHY 状态
        log::info!("DWC3: Checking USBDP PHY status");
        let _phy_status = self.phy.get_status();

        // 步骤 6: 配置 DWC3 PHY 寄存器（GUSB3PIPECTL、GUSB2PHYCFG）
        //
        // ⚠️ 必须在 xHCI HCRST 之后执行！
        // 此时 PHY 接口已经初始化，可以安全访问 PHY 配置寄存器。
        log::info!("DWC3: Configuring DWC3 PHY registers (after xHCI HCRST)");
        self.dwc_regs.setup_phy();

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
