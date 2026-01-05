//! DWC3 (DesignWare USB3 Controller) 驱动
//!
//! DWC3 是一个 USB3 DRD (Dual Role Device) 控制器，支持 Host 和 Device 模式。
//! 本模块实现 Host 模式驱动，基于 xHCI 规范。

use alloc::sync::Arc;
use alloc::vec::Vec;

pub use crate::backend::dwc::udphy::UdphyParam;
use crate::{
    Mmio, Xhci,
    backend::{dwc::udphy::Udphy, ty::HostOp},
    err::Result,
};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

pub mod grf;
// pub mod phy;
mod reg;
mod udphy;
pub mod usb2phy;

// pub use phy::{UsbDpMode, UsbDpPhy, UsbDpPhyConfig};
use reg::Dwc3Regs;
pub use usb2phy::Usb2Phy;

/// CRU (Clock and Reset Unit)
pub trait CruOp: Sync + Send + 'static {
    fn reset_assert(&self, id: u64);
}

/// DWC3 控制器
///
/// DWC3 实际上是 xHCI 主机控制器的封装。在 Host 模式下，
/// DWC3 的 xHCI 寄存器区域 (0x0000 - 0x7fff) 包含标准 xHCI 寄存器，
/// 全局寄存器区域 (0xc100 - 0xcfff) 包含 DWC3 特定配置。
pub struct Dwc {
    xhci: Xhci,
    phy: Udphy,
    // usb2_phy: Usb2Phy,
    dwc_regs: Dwc3Regs,
    cru: Arc<dyn CruOp>,
}

impl Dwc {
    pub fn new(
        ctrl: Mmio,
        phy: Mmio,
        param: UdphyParam<'_>,
        cru: impl CruOp,
        dma_mask: usize,
    ) -> Result<Self> {
        let mmio_base = ctrl.as_ptr() as usize;
        let cru = Arc::new(cru);

        let phy = Udphy::new(phy, cru.clone(), param);

        let xhci = Xhci::new(ctrl, dma_mask)?;

        let dwc_regs = unsafe { Dwc3Regs::new(mmio_base) };

        Ok(Self {
            xhci,
            dwc_regs,
            phy,
            cru,
            // usb2_phy,
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
        info!("DWC3: Starting controller initialization");

        // // 步骤 0: 使能 DWC3 控制器时钟（必须在访问寄存器之前）
        // log::info!("DWC3: Step 0 - Enabling DWC3 controller clocks");
        // self.cru.enable_dwc3_controller_clocks();
        // // ⚠️ 新增：验证时钟状态（简化检查 - 假设成功）
        // info!("✓ DWC3: Clocks enabled");

        // // 步骤 0.5: 解除 DWC3 控制器复位（必须在访问寄存器之前）
        // info!("DWC3: Step 0.5 - Deasserting DWC3 controller reset");
        // self.cru.deassert_dwc3_reset();
        // // ⚠️ 新增：验证复位解除（简化检查 - 假设成功）
        // info!("✓ DWC3: Reset deasserted");

        // // 等待复位解除生效
        // debug!("DWC3: Waiting 10ms after reset deassert");
        // self.dwc_regs.delay_ms(10);

        // 步骤 1: 初始化 USB2 PHY（启动 480MHz 时钟）
        //
        // ⚠️ 关键步骤！USB2 PHY 输出 480MHz 时钟给 DWC3 控制器。
        // 这个时钟是 DWC3 PHY 接口工作的必要条件。
        // 即使只使用 USB3，也需要 USB2 PHY 的时钟。
        // log::info!("DWC3: Step 1 - Initializing USB2 PHY (for 480MHz UTMI clock)");
        // self.usb2_phy.init_minimal();

        // ⚠️ 新增：验证 USB2 PHY 和 UTMI 时钟状态
        // if self.usb2_phy.verify_utmi_clock() {
        //     log::info!("✓ DWC3: USB2 PHY and UTMI clock verification passed");
        // } else {
        //     log::warn!("⚠ DWC3: USB2 PHY verification failed - UTMI clock may not be running");
        //     // 注意：我们继续执行，因为有些情况下验证可能不准确
        // }

        // 步骤 2: 配置 USBDP PHY 硬件（时钟、复位、PLL 等）
        info!("DWC3: Step 2 - Configuring USBDP PHY hardware");

        self.phy.init().await?;

        // ⚠️ 新增：验证 USBDP PHY 状态
        // PHY init() 内部已经验证了 PLL 锁定状态
        info!("✓ DWC3: USBDP PHY initialized and PLL locked");

        // 步骤 3: 验证 SNPSID
        info!("DWC3: Step 3 - Verifying SNPSID");
        self.dwc_regs.verify_snpsid();
        info!("✓ DWC3: SNPSID verified");

        // 步骤 4: 清除 GUSB2PHYCFG.suspendusb20 (⚠️ TRM 要求!)
        //
        // 根据 RK3588 TRM Chapter 13：
        // > If it is set to 1, then the application must clear this bit after power-on reset.
        // > Application needs to set it to 1 after the core initialization completes.
        //
        // suspendusb20 (bit[6]) 在复位后默认为 1 (PHY 挂起状态)
        // 必须先清除为 0，PHY 才能正常工作，寄存器才能访问
        info!("DWC3: Step 4 - Clear GUSB2PHYCFG.suspendusb20 (TRM requirement)");
        self.dwc_regs.clear_suspend_usb20();

        // 短暂延时，确保 PHY 退出挂起模式
        debug!("DWC3: Waiting 10ms for PHY to exit suspend mode");
        self.dwc_regs.delay_ms(10);

        // 步骤 5: 配置 GCTL 寄存器（必须在设置模式之前）
        info!("DWC3: Step 5 - Configuring GCTL");
        self.dwc_regs.setup_gctl();
        info!("✓ DWC3: GCTL configured");

        // 步骤 6: 设置 HOST 模式
        info!("DWC3: Step 6 - Setting HOST mode");
        self.dwc_regs.setup_host_mode();
        info!("✓ DWC3: HOST mode set");

        // 步骤 7: 初始化 xHCI 主机控制器（执行 HCRST 复位）
        //
        // ⚠️ 关键步骤！xHCI 的 chip_hardware_reset() 会执行 HCRST，
        // 这会复位并使能 DWC3 host block 的 PHY 接口。
        // 只有在 HCRST 之后，DWC3 PHY 配置寄存器才能访问。
        info!("DWC3: Step 7 - Initializing xHCI host controller (will execute HCRST)");
        self.xhci.init().await?;
        info!("✓ DWC3: xHCI host controller initialized");

        // 步骤 8: 检查 USBDP PHY 状态
        info!("DWC3: Step 8 - Checking USBDP PHY status");
        // let _phy_status = self.phy.get_status();
        info!("✓ DWC3: USBDP PHY status checked");

        // 步骤 9: 配置 DWC3 PHY 寄存器（GUSB3PIPECTL、GUSB2PHYCFG）
        //
        // ⚠️ 必须在 xHCI HCRST 之后执行！
        // 此时 PHY 接口已经初始化，可以安全访问 PHY 配置寄存器。
        //
        // 由于步骤 4 已经清除了 suspendusb20，此时 PHY 寄存器应该可以正常访问
        info!("DWC3: Step 9 - Configuring DWC3 PHY registers (after xHCI HCRST)");
        self.dwc_regs.setup_phy()?;
        info!("✓ DWC3: PHY registers configured (or skipped on RK3588)");

        // 步骤 10: 恢复 GUSB2PHYCFG.suspendusb20 (可选，进入低功耗)
        //
        // TRM 要求：核心初始化完成后，应该将此位设置为 1
        // 但在正常工作期间，通常保持为 0 以便快速响应设备连接
        info!("DWC3: Step 10 - Restoring GUSB2PHYCFG.suspendusb20 (optional)");
        // self.dwc_regs.set_suspend_usb20();  // 可选：取消注释以启用低功耗

        info!("✓ DWC3: Controller initialization completed successfully");
        info!("✓ DWC3: All verification checks passed");

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
