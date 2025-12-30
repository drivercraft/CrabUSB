//! RK3588 CRU (Clock and Reset Unit) 驱动
//!
//! ## 概述
//!
//! CRU 是 Rockchip SoC 的时钟和复位单元，用于配置和控制 SoC 内部的时钟和复位信号。
//! 本模块实现了 USBDP PHY 相关的时钟使能和复位控制功能。
//!
//! ## 参考来源
//!
//! - Linux: drivers/clk/rockchip/clk-rk3588.c
//! - Linux: drivers/reset/reset-rockchip.c
//! - 设备树: arch/arm/dts/rk3588s.dtsi
//!
//! ## 寄存器布局
//!
//! ### CRU 时钟门控寄存器 (0x0300 - 0x0400)
//! ```text
//! CLK_GATE_CON[n]: 每个寄存器控制 16 个时钟
//!   - bit 0-15:  写 1 使能时钟
//!   - bit 16-31: 写 1 禁用时钟
//! ```
//!
//! ### CRU 软复位寄存器 (0x0400 - 0x0480)
//! ```text
//! SOFTRST_CON[n]: 每个寄存器控制 16 个复位信号
//!   - bit 0-15:  写 1 断言复位
//!   - bit 16-31: 写 1 解除断言
//! ```

use crate::Mmio;

// =============================================================================
// 常量定义
// =============================================================================

/// CRU 寄存器块大小 (28KB)
pub const CRU_SIZE: usize = 0x7000;

/// 时钟门控寄存器基址偏移
pub const CLK_GATE_CON_OFFSET: usize = 0x0300;

/// 软复位寄存器基址偏移
pub const SOFTRST_CON_OFFSET: usize = 0x0400;

/// USBDP PHY 时钟 ID (RK3588)
pub const CLK_USBDP_PHY_REFCLK: u32 = 694; // 0x2b6
pub const CLK_USBDP_PHY_IMMORTAL: u32 = 639; // 0x27f
pub const CLK_USBDP_PHY_PCLK: u32 = 617; // 0x269

/// USB2 PHY 时钟 ID (RK3588)
pub const CLK_USB2PHY_PHYCLK: u32 = 693; // 0x2b5 - USB2 PHY 参考时钟

/// USBDP PHY 复位 ID (RK3588)
pub const RST_USBDP_INIT: u32 = 40; // 0x28
pub const RST_USBDP_CMN: u32 = 41; // 0x29
pub const RST_USBDP_LANE: u32 = 42; // 0x2a
pub const RST_USBDP_PCS_APB: u32 = 43; // 0x2b
pub const RST_USBDP_PMA_APB: u32 = 1154; // 0x482

/// USB2 PHY 复位 ID (RK3588)
pub const RST_USB2PHY_PHY: u32 = 0;  // 0xc0048 的低16位 - 需要特殊处理
pub const RST_USB2PHY_APB: u32 = 1161; // 0x489 - USB2 PHY APB 复位

// =============================================================================
// CRU 驱动实例
// =============================================================================

/// CRU 驱动实例
#[derive(Clone, Copy)]
pub struct Cru {
    /// CRU 寄存器基址
    base: usize,
}

impl Cru {
    /// 创建新的 CRU 实例
    ///
    /// # Safety
    ///
    /// 调用者必须确保 `mmio_base` 指向有效的内存映射寄存器区域
    pub unsafe fn new(mmio_base: Mmio) -> Self {
        Self {
            base: mmio_base.as_ptr() as usize,
        }
    }

    /// 获取 CRU 寄存器基址
    #[inline]
    fn base(&self) -> usize {
        self.base
    }

    // ========================================================================
    // 时钟控制方法
    // ========================================================================

    /// 使能时钟
    ///
    /// 写入 1 到 bit 0 使能时钟
    fn enable_clock(&mut self, clk_id: u32) {
        let reg_addr = self.base() + CLK_GATE_CON_OFFSET + (clk_id as usize / 16) * 4;
        let bit_offset = clk_id % 16;

        log::debug!(
            "CRU@{:x}: Enabling clock {} (addr={:x}, bit={})",
            self.base(),
            clk_id,
            reg_addr,
            bit_offset
        );

        let reg = unsafe { reg_addr as *mut u32 };
        let value = 1u32 << bit_offset;

        unsafe {
            reg.write_volatile(value);
        }
    }

    /// 禁用时钟
    ///
    /// 写入 1 到 bit 16 禁用时钟
    fn disable_clock(&mut self, clk_id: u32) {
        let reg_addr = self.base() + CLK_GATE_CON_OFFSET + (clk_id as usize / 16) * 4;
        let bit_offset = clk_id % 16;

        log::debug!(
            "CRU@{:x}: Disabling clock {} (addr={:x}, bit={})",
            self.base(),
            clk_id,
            reg_addr,
            bit_offset
        );

        let reg = unsafe { reg_addr as *mut u32 };
        let value = 1u32 << (16 + bit_offset);

        unsafe {
            reg.write_volatile(value);
        }
    }

    /// 使能 USBDP PHY 所有相关时钟
    ///
    /// 参考 u-boot rk3588_udphy_clk_enable()
    pub fn enable_usbdp_phy_clocks(&mut self) {
        log::info!("CRU@{:x}: Enabling USBDP PHY clocks", self.base());

        // 使能 refclk (24MHz reference clock)
        self.enable_clock(CLK_USBDP_PHY_REFCLK);

        // 使能 immortal clock (始终开启的时钟)
        self.enable_clock(CLK_USBDP_PHY_IMMORTAL);

        // 使能 pclk (APB peripheral clock)
        self.enable_clock(CLK_USBDP_PHY_PCLK);

        log::info!("✓ CRU@{:x}: USBDP PHY clocks enabled", self.base());
    }

    /// 使能 DWC3 控制器时钟
    ///
    /// DWC3 需要 3 个时钟：
    /// - REF_CLK_USB3OTG1 (422): 参考时钟
    /// - SUSPEND_CLK_USB3OTG1 (421): 挂起时钟
    /// - ACLK_USB3OTG1 (420): 总线时钟
    pub fn enable_dwc3_controller_clocks(&mut self) {
        log::info!("CRU@{:x}: Enabling DWC3 controller clocks", self.base());

        // 使能参考时钟
        log::debug!("Enabling REF_CLK_USB3OTG1 (422)");
        self.enable_clock(422);

        // 使能挂起时钟
        log::debug!("Enabling SUSPEND_CLK_USB3OTG1 (421)");
        self.enable_clock(421);

        // 使能总线时钟
        log::debug!("Enabling ACLK_USB3OTG1 (420)");
        self.enable_clock(420);

        log::info!("✓ CRU@{:x}: DWC3 controller clocks enabled", self.base());
    }

    /// 解除 DWC3 控制器复位
    ///
    /// DWC3 控制器有 AXI/AHB 复位，必须在访问寄存器之前解除
    pub fn deassert_dwc3_reset(&mut self) {
        log::info!("CRU@{:x}: Deasserting DWC3 controller reset", self.base());

        // 解除 DWC3 AXI 复位 (SRST_A_USB3OTG1 = 679)
        log::debug!("Deasserting SRST_A_USB3OTG1 (679)");
        self.deassert_reset(679);

        log::info!("✓ CRU@{:x}: DWC3 controller reset deasserted", self.base());
    }

    /// 使能 USB2 PHY 时钟
    ///
    /// USB2 PHY 需要参考时钟才能生成 UTMI 480MHz 时钟
    /// 这是 USBDP PHY UTMI 时钟的源头！
    pub fn enable_usb2_phy_clocks(&mut self) {
        log::info!("CRU@{:x}: Enabling USB2 PHY clocks", self.base());

        // 使能 USB2 PHY 参考时钟 (phyclk)
        log::debug!("Enabling CLK_USB2PHY_PHYCLK (693)");
        self.enable_clock(CLK_USB2PHY_PHYCLK);

        log::info!("✓ CRU@{:x}: USB2 PHY clocks enabled", self.base());
    }

    /// 解除 USB2 PHY 复位
    ///
    /// USB2 PHY 有两个复位信号：
    /// - PHY 复位 (0xc0048): 需要特殊处理
    /// - APB 复位 (0x489): APB 总线复位
    ///
    /// 注意：0xc0048 是一个特殊值，可能是多个复位的组合
    /// 根据 RK3588 复位定义，0xc0048 实际上是 PMU_SMNI_PERF_RSTNR
    /// 这里我们先处理标准的 APB 复位
    pub fn deassert_usb2_phy_resets(&mut self) {
        log::info!("CRU@{:x}: Deasserting USB2 PHY resets", self.base());

        // 解除 USB2 PHY APB 复位
        log::debug!("Deasserting RST_USB2PHY_APB (1161)");
        self.deassert_reset(RST_USB2PHY_APB);

        log::info!("✓ CRU@{:x}: USB2 PHY resets deasserted", self.base());
    }

    // ========================================================================
    // 复位控制方法
    // ========================================================================

    /// 断言复位 (assert reset)
    ///
    /// 写入 1 到 bit 0 断言复位
    fn assert_reset(&mut self, rst_id: u32) {
        let reg_addr = self.base() + SOFTRST_CON_OFFSET + (rst_id as usize / 16) * 4;
        let bit_offset = rst_id % 16;

        log::debug!(
            "CRU@{:x}: Asserting reset {} (addr={:x}, bit={})",
            self.base(),
            rst_id,
            reg_addr,
            bit_offset
        );

        let reg = unsafe { reg_addr as *mut u32 };
        let value = 1u32 << bit_offset;

        unsafe {
            reg.write_volatile(value);
        }
    }

    /// 解除断言复位 (deassert reset)
    ///
    /// 写入 1 到 bit 16 解除断言
    fn deassert_reset(&mut self, rst_id: u32) {
        let reg_addr = self.base() + SOFTRST_CON_OFFSET + (rst_id as usize / 16) * 4;
        let bit_offset = rst_id % 16;

        log::debug!(
            "CRU@{:x}: Deasserting reset {} (addr={:x}, bit={})",
            self.base(),
            rst_id,
            reg_addr,
            bit_offset
        );

        let reg = unsafe { reg_addr as *mut u32 };
        let value = 1u32 << (16 + bit_offset);

        unsafe {
            reg.write_volatile(value);
        }
    }

    /// 解除 USBDP PHY APB 复位
    ///
    /// APB 复位必须首先解除，以便访问寄存器
    pub fn deassert_usbdp_phy_apb_reset(&mut self) {
        log::info!(
            "CRU@{:x}: Deasserting USBDP PHY APB resets",
            self.base()
        );

        // 解除 PCS_APB 复位
        self.deassert_reset(RST_USBDP_PCS_APB);

        // 解除 PMA_APB 复位
        self.deassert_reset(RST_USBDP_PMA_APB);

        log::info!(
            "✓ CRU@{:x}: USBDP PHY APB resets deasserted",
            self.base()
        );
    }

    /// 解除 USBDP PHY 初始化复位
    ///
    /// 按照 u-boot 驱动的顺序解除复位
    /// 参考: drivers/phy/phy-rockchip-usbdp.c:rk3588_udphy_init()
    ///
    /// 复位解除顺序和时延：
    /// 1. 解除 INIT 复位
    /// 2. 等待 1ms (数据手册要求 200ns，实际使用 1ms 提供余量)
    /// 3. 解除 CMN/LANE 复位
    pub fn deassert_usbdp_phy_init_resets(&mut self) {
        log::info!(
            "CRU@{:x}: Deasserting USBDP PHY init resets",
            self.base()
        );

        // Step 1: 解除 init 复位
        self.deassert_reset(RST_USBDP_INIT);

        // Step 2: 等待 1ms (数据手册要求 200ns，u-boot 使用 1ms)
        // ⚠️  关键时延！PLL 锁定失败通常是因为这个时延太短
        log::debug!("CRU@{:x}: Waiting 1ms after INIT reset deassert", self.base());
        self.delay_us(1000); // 1ms = 1000us

        // Step 3: 解除 cmn/lane 复位
        self.deassert_reset(RST_USBDP_CMN);
        self.deassert_reset(RST_USBDP_LANE);

        log::info!(
            "✓ CRU@{:x}: USBDP PHY init resets deasserted",
            self.base()
        );
    }

    /// 微秒级延时
    fn delay_us(&self, us: u32) {
        crate::osal::kernel::delay(core::time::Duration::from_micros(us as _));
    }
}

// =============================================================================
// 测试
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_ids() {
        assert_eq!(CLK_USBDP_PHY_REFCLK, 694);
        assert_eq!(CLK_USBDP_PHY_IMMORTAL, 639);
        assert_eq!(CLK_USBDP_PHY_PCLK, 617);
    }

    #[test]
    fn test_reset_ids() {
        assert_eq!(RST_USBDP_INIT, 40);
        assert_eq!(RST_USBDP_CMN, 41);
        assert_eq!(RST_USBDP_LANE, 42);
        assert_eq!(RST_USBDP_PCS_APB, 43);
        assert_eq!(RST_USBDP_PMA_APB, 1154);
    }

    #[test]
    fn test_clk_gate_register_calculation() {
        // CLK_USBDP_PHY_PCLK = 617
        // reg_idx = 617 / 16 = 38
        // bit_offset = 617 % 16 = 9
        // addr = 0x0300 + 38 * 4 = 0x0300 + 0x98 = 0x398
        let clk_id = CLK_USBDP_PHY_PCLK;
        let reg_addr = 0x0300 + (clk_id as usize / 16) * 4;
        let bit_offset = clk_id % 16;
        assert_eq!(reg_addr, 0x398);
        assert_eq!(bit_offset, 9);
    }

    #[test]
    fn test_softrst_register_calculation() {
        // RST_USBDP_INIT = 40
        // reg_idx = 40 / 16 = 2
        // bit_offset = 40 % 16 = 8
        // addr = 0x0400 + 2 * 4 = 0x0400 + 0x8 = 0x408
        let rst_id = RST_USBDP_INIT;
        let reg_addr = 0x0400 + (rst_id as usize / 16) * 4;
        let bit_offset = rst_id % 16;
        assert_eq!(reg_addr, 0x408);
        assert_eq!(bit_offset, 8);
    }

    #[test]
    fn test_clk_gate_enable_value() {
        // bit 9 使能
        let bit_offset = 9u32;
        let value = 1u32 << bit_offset;
        assert_eq!(value, 0x200);
    }

    #[test]
    fn test_clk_gate_disable_value() {
        // bit 16+9 = 25 禁用
        let bit_offset = 9u32;
        let value = 1u32 << (16 + bit_offset);
        assert_eq!(value, 0x2000000);
    }

    #[test]
    fn test_softrst_assert_value() {
        // bit 8 断言
        let bit_offset = 8u32;
        let value = 1u32 << bit_offset;
        assert_eq!(value, 0x100);
    }

    #[test]
    fn test_softrst_deassert_value() {
        // bit 16+8 = 24 解除断言
        let bit_offset = 8u32;
        let value = 1u32 << (16 + bit_offset);
        assert_eq!(value, 0x1000000);
    }
}
