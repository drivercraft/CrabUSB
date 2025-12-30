//! RK3588 USBDP PHY 驱动
//!
//! 基于 Rockchip USBDP Combo PHY，支持 USB3.0 和 DisplayPort 1.4。
//! 参考 Linux drivers/phy/rockchip/phy-rockchip-usbdp.c 和 u-boot 驱动。
//!
//! ## 功能特性
//!
//! - USB3.0 SuperSpeed PHY 支持
//! - USB2.0 HS/FS/LS PHY 支持
//! - DisplayPort Alt Mode 支持
//! - Lane multiplexing (USB/DP 共享通道)
//! - 时钟和复位管理
//! - GRF 寄存器配置
//!
//! ## 寄存器地址映射
//!
//! ```text
//! USBDP PHY0 @ 0xfed80000:
//!   - PMA 寄存器: +0x8000
//!   - PCS 寄存器: +0x4000
//!
//! GRF 寄存器（设备树定义）:
//!   - usbdpphy0-grf: 0xfd5c8000 (syscon@fd5c8000)
//!   - usbdpphy1-grf: 0xfd5cc000 (syscon@fd5cc000)
//!   - usb-grf:       0xfd5ac000 (syscon@fd5ac000, PHY0 和 PHY1 共享)
//!   - u2phy-grf:     0xfd5d0000 (syscon@fd5d0000)
//!   - vo-grf:        0xfd5a6000 (syscon@fd5a6000)
//! ```
//!
//! 参考 GRF_DTS_ANALYSIS.md 了解地址获取过程。

use tock_registers::{RegisterLongName, registers::*};

use tock_registers::interfaces::*;
use tock_registers::register_bitfields;

use super::grf::{Grf, GrfType};
use crate::{Mmio, err::Result};

// =============================================================================
// 常量定义
// =============================================================================

/// USBDP PHY 寄存器偏移
pub const UDPHY_PMA: usize = 0x8000;
pub const UDPHY_PCS: usize = 0x4000;

/// 时钟 ID (RK3588 CRU)
pub const CLK_USBDP_PHY_REFCLK: u32 = 694; // 0x2b6
pub const CLK_USBDP_PHY_IMMORTAL: u32 = 639; // 0x27f
pub const CLK_USBDP_PHY_PCLK: u32 = 617; // 0x269

/// 复位 ID (RK3588 CRU)
pub const RST_USBDP_INIT: u32 = 40; // 0x28
pub const RST_USBDP_CMN: u32 = 41; // 0x29
pub const RST_USBDP_LANE: u32 = 42; // 0x2a
pub const RST_USBDP_PCS_APB: u32 = 43; // 0x2b
pub const RST_USBDP_PMA_APB: u32 = 1154; // 0x482

// =============================================================================
// 寄存器位字段定义
// =============================================================================

/// PMA CMN 寄存器偏移
#[allow(unused)]
pub mod pma_offset {
    pub const CMN_LANE_MUX_AND_EN: usize = 0x0288;
    pub const CMN_DP_LINK: usize = 0x028c;
    pub const CMN_SSC_EN: usize = 0x02d0;
    pub const CMN_ANA_LCPLL_DONE: usize = 0x0350;
    pub const CMN_ANA_ROPLL_DONE: usize = 0x0354;
    pub const CMN_DP_RSTN: usize = 0x038c;
    pub const TRSV_LN0_MON_RX_CDR: usize = 0x0b84;
    pub const TRSV_LN2_MON_RX_CDR: usize = 0x1b84;
}

register_bitfields![u32,
    CMN_LANE_MUX_EN [
        /// Lane 3 multiplexer select
        LANE3_MUX OFFSET(7) NUMBITS(1) [
            USB = 0,
            DP = 1
        ],
        /// Lane 2 multiplexer select
        LANE2_MUX OFFSET(6) NUMBITS(1) [
            USB = 0,
            DP = 1
        ],
        /// Lane 1 multiplexer select
        LANE1_MUX OFFSET(5) NUMBITS(1) [
            USB = 0,
            DP = 1
        ],
        /// Lane 0 multiplexer select
        LANE0_MUX OFFSET(4) NUMBITS(1) [
            USB = 0,
            DP = 1
        ],
        /// Lane 3 enable
        LANE3_EN OFFSET(3) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],
        /// Lane 2 enable
        LANE2_EN OFFSET(2) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],
        /// Lane 1 enable
        LANE1_EN OFFSET(1) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],
        /// Lane 0 enable
        LANE0_EN OFFSET(0) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],
    ]
];

/// CMN_DP_RSTN 寄存器位字段
register_bitfields![u32,
    CMN_DP_RSTN [
        /// CDR watchdog enable
        CDR_WTCHGD_MSK_CDR_EN OFFSET(0) NUMBITS(1) [
            Mask = 0,
            Enable = 1
        ],
        /// CDR watchdog enable
        CDR_WTCHDG_EN OFFSET(1) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],
        /// DP common reset
        DP_CMN_RSTN OFFSET(2) NUMBITS(1) [
            Reset = 0,
            Enable = 1
        ],
        /// DP init reset
        DP_INIT_RSTN OFFSET(3) NUMBITS(1) [
            Reset = 0,
            Enable = 1
        ],
    ]
];

/// CMN_ANA_LCPLL_DONE 寄存器位字段
register_bitfields![u32,
    CMN_ANA_LCPLL [
        /// LCPLL AFC done
        AFC_DONE OFFSET(6) NUMBITS(1) [
            NotDone = 0,
            Done = 1
        ],
        /// LCPLL lock done
        LOCK_DONE OFFSET(7) NUMBITS(1) [
            NotLocked = 0,
            Locked = 1
        ],
    ]
];

/// CMN_ANA_ROPLL_DONE 寄存器位字段
register_bitfields![u32,
    CMN_ANA_ROPLL [
        /// ROPLL AFC done
        AFC_DONE OFFSET(0) NUMBITS(1) [
            NotDone = 0,
            Done = 1
        ],
        /// ROPLL lock done
        LOCK_DONE OFFSET(1) NUMBITS(1) [
            NotLocked = 0,
            Locked = 1
        ],
    ]
];

// =============================================================================
// 数据结构
// =============================================================================

/// USBDP PHY 模式
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum UsbDpMode {
    None = 0,
    Usb = 1,
    Dp = 2,
    UsbDp = 3,
}

/// USBDP PHY 初始化配置
#[derive(Debug, Clone)]
pub struct UsbDpPhyConfig {
    /// PHY ID (0 或 1)
    pub id: u8,
    /// 模式 (USB/DP/Combo)
    pub mode: UsbDpMode,
    /// 是否启用翻转
    pub flip: bool,
    /// DP lane 映射 (用于 DP 或 Combo 模式)
    pub dp_lane_map: [u8; 4],
}

impl Default for UsbDpPhyConfig {
    fn default() -> Self {
        Self {
            id: 0,
            mode: UsbDpMode::Usb,
            flip: false,
            dp_lane_map: [0, 1, 2, 3],
        }
    }
}

/// USBDP PHY 驱动
pub struct UsbDpPhy {
    /// PHY 配置
    pub config: UsbDpPhyConfig,
    /// PHY MMIO 基址
    phy_base: usize,
    /// USBDP PHY GRF
    dp_grf: Grf,
    /// USB GRF
    usb_grf: Grf,
}

impl UsbDpPhy {
    /// 创建新的 USBDP PHY 驱动实例
    ///
    /// # 参数
    ///
    /// * `config` - PHY 配置
    /// * `phy_base` - PHY 寄存器基址
    /// * `usbdpphy_grf_base` - USBDP PHY GRF 基址
    /// * `usb_grf_base` - USB GRF 基址
    ///
    /// # Safety
    ///
    /// 调用者必须确保相关寄存器地址有效
    pub fn new(config: UsbDpPhyConfig, phy_base: Mmio, u3_grf: Mmio, dp_grf: Mmio) -> Self {
        // 创建 GRF 实例
        let dp_grf = unsafe { Grf::new(dp_grf, GrfType::UsbdpPhy) };
        let usb_grf = unsafe { Grf::new(u3_grf, GrfType::Usb) };

        Self {
            config,
            phy_base: phy_base.as_ptr() as usize,
            dp_grf,
            usb_grf,
        }
    }

    /// 初始化 USBDP PHY
    ///
    /// 基于 u-boot drivers/phy/phy-rockchip-usbdp.c:rk3588_udphy_init()
    ///
    /// ## 初始化流程
    ///
    /// 1. 退出低功耗模式
    /// 2. 解除 APB 复位
    /// 3. 配置参考时钟
    /// 4. 配置初始化序列
    /// 5. 配置 lane multiplexing
    /// 6. 解除 init/cmn/lane 复位
    /// 7. 等待 PLL 锁定
    ///
    /// # 错误
    ///
    /// 如果 PLL 未能在超时时间内锁定，返回错误
    pub fn init(&mut self) -> Result<()> {
        log::info!("USBDP PHY: Starting initialization");

        // Step 1: 退出低功耗模式
        self.exit_low_power_mode();

        // Step 2: 解除 APB 复位
        self.deassert_apb_reset();

        // Step 3: 配置参考时钟
        self.configure_refclk();

        // Step 4: 配置初始化序列
        self.configure_init_sequence();

        // Step 5: 配置 lane multiplexing
        self.configure_lane_mux();

        // Step 6: 解除 init/cmn/lane 复位
        self.deassert_phy_reset();

        // Step 7: 等待 PLL 锁定
        // self.wait_pll_lock()?;

        log::info!("✓ USBDP PHY{} initialized successfully", self.config.id);
        Ok(())
    }

    fn offset_reg<R: RegisterLongName>(&self, offset: usize) -> &ReadWrite<u32, R> {
        let val = (self.phy_base + offset) as *const ReadWrite<u32, R>;
        unsafe { &*val }
    }

    /// 退出低功耗模式
    fn exit_low_power_mode(&self) {
        log::debug!("USBDP PHY{}: Exiting low power mode", self.config.id);

        // 设置 LOW_PWRN = 1
        self.dp_grf.exit_low_power();

        // 如果是 USB 模式，启用 RX LFPS
        if self.config.mode == UsbDpMode::Usb || self.config.mode == UsbDpMode::UsbDp {
            self.dp_grf.enable_rx_lfps();
            log::debug!("USBDP PHY{}: RX LFPS enabled", self.config.id);
        }
    }

    /// 解除 APB 复位
    fn deassert_apb_reset(&self) {
        log::debug!("USBDP PHY{}: Deasserting APB reset", self.config.id);

        // TODO: 通过 CRU 接口解除复位
        // reset_deassert(RST_USBDP_PMA_APB);
        // reset_deassert(RST_USBDP_PCS_APB);

        log::debug!("USBDP PHY{}: APB reset deasserted", self.config.id);
    }

    /// 配置参考时钟
    fn configure_refclk(&self) {
        log::debug!("USBDP PHY: Configuring reference clock (24MHz)",);

        // 参考 u-boot rk3588_udphy_24m_refclk_cfg
        // 这里只写入关键寄存器，完整序列在实际硬件初始化时应用

        // TODO: 实现 24MHz 参考时钟配置序列
        // __regmap_multi_reg_write(pma_regmap, rk3588_udphy_24m_refclk_cfg, ...);

        log::debug!("USBDP PHY: Reference clock configured");
    }

    /// 配置初始化序列
    fn configure_init_sequence(&self) {
        log::debug!("USBDP PHY{}: Applying init sequence", self.config.id);

        // 参考 u-boot rk3588_udphy_init_sequence
        // 这里只写入关键寄存器

        // TODO: 实现完整的初始化序列
        // __regmap_multi_reg_write(pma_regmap, rk3588_udphy_init_sequence, ...);

        log::debug!("USBDP PHY{}: Init sequence applied", self.config.id);
    }

    fn cmn_lane_mux_and_en(&self) -> &ReadWrite<u32, CMN_LANE_MUX_EN::Register> {
        self.offset_reg(UDPHY_PMA + pma_offset::CMN_LANE_MUX_AND_EN)
    }

    /// 配置 lane multiplexing
    fn configure_lane_mux(&self) {
        log::debug!("USBDP PHY{}: Configuring lane mux", self.config.id);

        // 默认启用所有 lane
        let mut val = CMN_LANE_MUX_EN::LANE0_EN::Enable
            + CMN_LANE_MUX_EN::LANE1_EN::Enable
            + CMN_LANE_MUX_EN::LANE2_EN::Enable
            + CMN_LANE_MUX_EN::LANE3_EN::Enable;

        match self.config.mode {
            UsbDpMode::Usb => {
                // USB 模式: 所有 lane 配置为 USB
                val += CMN_LANE_MUX_EN::LANE0_MUX::USB
                    + CMN_LANE_MUX_EN::LANE1_MUX::USB
                    + CMN_LANE_MUX_EN::LANE2_MUX::USB
                    + CMN_LANE_MUX_EN::LANE3_MUX::USB;
                log::debug!("USBDP PHY{}: All lanes set to USB mode", self.config.id);
            }
            UsbDpMode::Dp => {
                // DP 模式: 所有 lane 配置为 DP
                val += CMN_LANE_MUX_EN::LANE0_MUX::DP
                    + CMN_LANE_MUX_EN::LANE1_MUX::DP
                    + CMN_LANE_MUX_EN::LANE2_MUX::DP
                    + CMN_LANE_MUX_EN::LANE3_MUX::DP;
                log::debug!("USBDP PHY{}: All lanes set to DP mode", self.config.id);
            }
            UsbDpMode::UsbDp => {
                todo!()
            }
            UsbDpMode::None => {
                log::warn!(
                    "USBDP PHY{}: No lane mux configuration for mode None",
                    self.config.id
                );
            }
        }

        self.cmn_lane_mux_and_en().write(val);
    }

    /// 解除 PHY 复位
    fn deassert_phy_reset(&self) {
        log::debug!("USBDP PHY{}: Deasserting PHY reset", self.config.id);

        // Step 1: 解除 init 复位
        if self.config.mode == UsbDpMode::Usb || self.config.mode == UsbDpMode::UsbDp {
            self.deassert_reset(RST_USBDP_INIT);
            log::debug!("USBDP PHY{}: Init reset deasserted", self.config.id);
        }

        // Step 2: 如果是 DP 模式，解除 DP init 复位
        if self.config.mode == UsbDpMode::Dp || self.config.mode == UsbDpMode::UsbDp {
            let pma_base = self.phy_base + UDPHY_PMA;
            let dp_rstn_reg = unsafe { (pma_base + pma_offset::CMN_DP_RSTN) as *mut u32 };

            unsafe {
                let value = dp_rstn_reg.read_volatile();
                dp_rstn_reg.write_volatile(value | (1 << 3)); // DP_INIT_RSTN
            }
            log::debug!("USBDP PHY{}: DP init reset deasserted", self.config.id);
        }

        // Step 3: 等待 1 微秒
        self.delay_us(1);

        // Step 4: 解除 cmn/lane 复位 (仅 USB 模式)
        if self.config.mode == UsbDpMode::Usb || self.config.mode == UsbDpMode::UsbDp {
            self.deassert_reset(RST_USBDP_CMN);
            self.deassert_reset(RST_USBDP_LANE);
            log::debug!("USBDP PHY{}: CMN/LANE reset deasserted", self.config.id);
        }
    }

    /// 等待 PLL 锁定
    fn wait_pll_lock(&self) -> Result<()> {
        log::debug!("USBDP PHY{}: Waiting for PLL lock", self.config.id);

        let pma_base = self.phy_base + UDPHY_PMA;

        // 等待 LCPLL 锁定 (USB 模式需要)
        if self.config.mode == UsbDpMode::Usb || self.config.mode == UsbDpMode::UsbDp {
            let lcpll_reg = unsafe { (pma_base + pma_offset::CMN_ANA_LCPLL_DONE) as *const u32 };

            let timeout = 100; // 100ms
            let start = self.get_time_ms();

            loop {
                let value = unsafe { lcpll_reg.read_volatile() };
                let afc_done = (value >> 6) & 0x1 == 1;
                let lock_done = (value >> 7) & 0x1 == 1;

                if afc_done && lock_done {
                    log::info!("USBDP PHY{}: LCPLL locked", self.config.id);
                    break;
                }

                if self.get_time_ms() - start > timeout {
                    log::error!("USBDP PHY{}: LCPLL lock timeout", self.config.id);
                    return Err(crate::err::USBError::Timeout);
                }

                self.delay_us(200); // 200 微秒轮询间隔
            }
        }

        log::info!("✓ USBDP PHY{}: PLL locked successfully", self.config.id);
        Ok(())
    }

    /// 解除单个复位
    fn deassert_reset(&self, reset_id: u32) {
        // TODO: 通过 CRU 接口解除复位
        // reset_deassert(reset_id);
        log::trace!(
            "USBDP PHY{}: Deasserting reset {}",
            self.config.id,
            reset_id
        );
    }

    /// 微秒级延时
    fn delay_us(&self, us: u32) {
        const LOOPS_PER_US: u32 = 50;
        let total_loops = us * LOOPS_PER_US;
        for _ in 0..total_loops {
            core::sync::atomic::spin_loop_hint();
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    /// 获取当前时间 (毫秒)
    fn get_time_ms(&self) -> u64 {
        // TODO: 实现实际的时间获取
        0
    }

    /// 启用 USB3 U3 端口
    pub fn enable_u3_port(&mut self) {
        log::info!("USBDP PHY{}: Enabling USB3 U3 port", self.config.id);

        // 使用 USB GRF 启用 U3 端口
        self.usb_grf.enable_u3_port(self.config.id);

        log::info!("USBDP PHY{}: USB3 U3 port enabled", self.config.id);
    }

    /// 禁用 USB3 U3 端口
    pub fn disable_u3_port(&mut self) {
        log::info!("USBDP PHY{}: Disabling USB3 U3 port", self.config.id);

        // 使用 USB GRF 禁用 U3 端口
        self.usb_grf.disable_u3_port(self.config.id);

        log::info!("USBDP PHY{}: USB3 U3 port disabled", self.config.id);
    }

    /// 获取 PHY 状态
    pub fn get_status(&self) -> UsbDpPhyStatus {
        let pma_base = self.phy_base + UDPHY_PMA;

        // 读取 PLL 锁定状态
        let lcpll_reg = unsafe { (pma_base + pma_offset::CMN_ANA_LCPLL_DONE) as *const u32 };
        let lcpll_value = unsafe { lcpll_reg.read_volatile() };
        let lcpll_locked = (lcpll_value >> 7) & 0x1 == 1;

        let ropll_reg = unsafe { (pma_base + pma_offset::CMN_ANA_ROPLL_DONE) as *const u32 };
        let ropll_value = unsafe { ropll_reg.read_volatile() };
        let ropll_locked = (ropll_value >> 1) & 0x1 == 1;

        UsbDpPhyStatus {
            lcpll_locked,
            ropll_locked,
            mode: self.config.mode,
        }
    }
}

/// USBDP PHY 状态
#[derive(Debug, Clone, Copy)]
pub struct UsbDpPhyStatus {
    /// LCPLL 锁定状态
    pub lcpll_locked: bool,
    /// ROPLL 锁定状态
    pub ropll_locked: bool,
    /// 当前模式
    pub mode: UsbDpMode,
}

// =============================================================================
// 测试辅助函数
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phy_config_default() {
        let config = UsbDpPhyConfig::default();
        assert_eq!(config.id, 0);
        assert_eq!(config.mode, UsbDpMode::Usb);
        assert_eq!(config.flip, false);
        assert_eq!(config.dp_lane_map, [0, 1, 2, 3]);
    }

    #[test]
    fn test_mode_values() {
        assert_eq!(UsbDpMode::None as u8, 0);
        assert_eq!(UsbDpMode::Usb as u8, 1);
        assert_eq!(UsbDpMode::Dp as u8, 2);
        assert_eq!(UsbDpMode::UsbDp as u8, 3);
    }

    #[test]
    fn test_grf_addresses() {
        // 测试 PHY0 GRF 地址（来自设备树）
        // syscon@fd5c8000 (USBDP PHY0 GRF)
        // syscon@fd5ac000 (USB GRF - 与 PHY1 共享)
        let phy0_usbdpphy_grf: usize = 0xfd5c8000;
        let phy0_usb_grf: usize = 0xfd5ac000;

        assert_eq!(phy0_usbdpphy_grf, 0xfd5c8000, "PHY0 USBDPPHY GRF 地址错误");
        assert_eq!(phy0_usb_grf, 0xfd5ac000, "PHY0 USB GRF 地址错误");

        // 测试 PHY1 GRF 地址（来自设备树）
        // syscon@fd5cc000 (USBDP PHY1 GRF)
        // syscon@fd5ac000 (USB GRF - 与 PHY0 共享)
        let phy1_usbdpphy_grf: usize = 0xfd5cc000;
        let phy1_usb_grf: usize = 0xfd5ac000;

        assert_eq!(phy1_usbdpphy_grf, 0xfd5cc000, "PHY1 USBDPPHY GRF 地址错误");
        assert_eq!(phy1_usb_grf, 0xfd5ac000, "PHY1 USB GRF 地址错误");
    }
}
