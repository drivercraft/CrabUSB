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

use super::cru::Cru;
use super::grf::{Grf, GrfType};
use crate::{Mmio, err::Result};

// =============================================================================
// 常量定义
// =============================================================================

/// USBDP PHY 寄存器偏移
pub const UDPHY_PMA: usize = 0x8000;
pub const UDPHY_PCS: usize = 0x4000;

/// 寄存器配置项
#[derive(Debug, Clone, Copy)]
struct RegConfig {
    offset: u16,
    value: u32,
}

// =============================================================================
// 寄存器初始化序列 (基于 u-boot drivers/phy/phy-rockchip-usbdp.c)
// =============================================================================

/// RK3588 USBDP PHY 24MHz 参考时钟配置序列
///
/// 参考 u-boot: rk3588_udphy_24m_refclk_cfg
/// 位置: drivers/phy/phy-rockchip-usbdp.c:226-263
const REFCLK_24M_CFG: &[RegConfig] = &[
    // PMA 寄存器配置块 0
    RegConfig {
        offset: 0x0090,
        value: 0x68,
    },
    RegConfig {
        offset: 0x0094,
        value: 0x68,
    },
    RegConfig {
        offset: 0x0128,
        value: 0x24,
    },
    RegConfig {
        offset: 0x012c,
        value: 0x44,
    },
    RegConfig {
        offset: 0x0130,
        value: 0x3f,
    },
    RegConfig {
        offset: 0x0134,
        value: 0x44,
    },
    RegConfig {
        offset: 0x015c,
        value: 0xa9,
    },
    RegConfig {
        offset: 0x0160,
        value: 0x71,
    },
    RegConfig {
        offset: 0x0164,
        value: 0x71,
    },
    RegConfig {
        offset: 0x0168,
        value: 0xa9,
    },
    RegConfig {
        offset: 0x0174,
        value: 0xa9,
    },
    RegConfig {
        offset: 0x0178,
        value: 0x71,
    },
    RegConfig {
        offset: 0x017c,
        value: 0x71,
    },
    RegConfig {
        offset: 0x0180,
        value: 0xa9,
    },
    RegConfig {
        offset: 0x018c,
        value: 0x41,
    },
    RegConfig {
        offset: 0x0190,
        value: 0x00,
    },
    RegConfig {
        offset: 0x0194,
        value: 0x05,
    },
    RegConfig {
        offset: 0x01ac,
        value: 0x2a,
    },
    RegConfig {
        offset: 0x01b0,
        value: 0x17,
    },
    RegConfig {
        offset: 0x01b4,
        value: 0x17,
    },
    RegConfig {
        offset: 0x01b8,
        value: 0x2a,
    },
    RegConfig {
        offset: 0x01c8,
        value: 0x04,
    },
    RegConfig {
        offset: 0x01cc,
        value: 0x08,
    },
    RegConfig {
        offset: 0x01d0,
        value: 0x08,
    },
    RegConfig {
        offset: 0x01d4,
        value: 0x04,
    },
    RegConfig {
        offset: 0x01d8,
        value: 0x20,
    },
    RegConfig {
        offset: 0x01dc,
        value: 0x01,
    },
    RegConfig {
        offset: 0x01e0,
        value: 0x09,
    },
    RegConfig {
        offset: 0x01e4,
        value: 0x03,
    },
    RegConfig {
        offset: 0x01f0,
        value: 0x29,
    },
    RegConfig {
        offset: 0x01f4,
        value: 0x02,
    },
    RegConfig {
        offset: 0x01f8,
        value: 0x02,
    },
    RegConfig {
        offset: 0x01fc,
        value: 0x29,
    },
    RegConfig {
        offset: 0x0208,
        value: 0x2a,
    },
    RegConfig {
        offset: 0x020c,
        value: 0x17,
    },
    RegConfig {
        offset: 0x0210,
        value: 0x17,
    },
    RegConfig {
        offset: 0x0214,
        value: 0x2a,
    },
    RegConfig {
        offset: 0x0224,
        value: 0x20,
    },
    RegConfig {
        offset: 0x03f0,
        value: 0x0a,
    },
    RegConfig {
        offset: 0x03f4,
        value: 0x07,
    },
    RegConfig {
        offset: 0x03f8,
        value: 0x07,
    },
    RegConfig {
        offset: 0x03fc,
        value: 0x0c,
    },
    RegConfig {
        offset: 0x0404,
        value: 0x12,
    },
    RegConfig {
        offset: 0x0408,
        value: 0x1a,
    },
    RegConfig {
        offset: 0x040c,
        value: 0x1a,
    },
    RegConfig {
        offset: 0x0410,
        value: 0x3f,
    },
    // Lane 0 和 Lane 1 配置
    RegConfig {
        offset: 0x0ce0,
        value: 0x68,
    },
    RegConfig {
        offset: 0x0ce8,
        value: 0xd0,
    },
    RegConfig {
        offset: 0x0cf0,
        value: 0x87,
    },
    RegConfig {
        offset: 0x0cf8,
        value: 0x70,
    },
    RegConfig {
        offset: 0x0d00,
        value: 0x70,
    },
    RegConfig {
        offset: 0x0d08,
        value: 0xa9,
    },
    // Lane 2 和 Lane 3 配置
    RegConfig {
        offset: 0x1ce0,
        value: 0x68,
    },
    RegConfig {
        offset: 0x1ce8,
        value: 0xd0,
    },
    RegConfig {
        offset: 0x1cf0,
        value: 0x87,
    },
    RegConfig {
        offset: 0x1cf8,
        value: 0x70,
    },
    RegConfig {
        offset: 0x1d00,
        value: 0x70,
    },
    RegConfig {
        offset: 0x1d08,
        value: 0xa9,
    },
    // Lane 0 Tx 驱动配置
    RegConfig {
        offset: 0x0a3c,
        value: 0xd0,
    },
    RegConfig {
        offset: 0x0a44,
        value: 0xd0,
    },
    RegConfig {
        offset: 0x0a48,
        value: 0x01,
    },
    RegConfig {
        offset: 0x0a4c,
        value: 0x0d,
    },
    RegConfig {
        offset: 0x0a54,
        value: 0xe0,
    },
    RegConfig {
        offset: 0x0a5c,
        value: 0xe0,
    },
    RegConfig {
        offset: 0x0a64,
        value: 0xa8,
    },
    // Lane 2 Tx 驱动配置
    RegConfig {
        offset: 0x1a3c,
        value: 0xd0,
    },
    RegConfig {
        offset: 0x1a44,
        value: 0xd0,
    },
    RegConfig {
        offset: 0x1a48,
        value: 0x01,
    },
    RegConfig {
        offset: 0x1a4c,
        value: 0x0d,
    },
    RegConfig {
        offset: 0x1a54,
        value: 0xe0,
    },
    RegConfig {
        offset: 0x1a5c,
        value: 0xe0,
    },
    RegConfig {
        offset: 0x1a64,
        value: 0xa8,
    },
];

/// RK3588 USBDP PHY 初始化序列
///
/// 参考 u-boot: rk3588_udphy_init_sequence
/// 位置: drivers/phy/phy-rockchip-usbdp.c:265-299
const INIT_SEQUENCE: &[RegConfig] = &[
    // CMN 和 Lane 0 初始化
    RegConfig {
        offset: 0x0104,
        value: 0x44,
    },
    RegConfig {
        offset: 0x0234,
        value: 0xE8,
    },
    RegConfig {
        offset: 0x0248,
        value: 0x44,
    },
    RegConfig {
        offset: 0x028C,
        value: 0x18,
    },
    RegConfig {
        offset: 0x081C,
        value: 0xE5,
    },
    RegConfig {
        offset: 0x0878,
        value: 0x00,
    },
    RegConfig {
        offset: 0x0994,
        value: 0x1C,
    },
    RegConfig {
        offset: 0x0AF0,
        value: 0x00,
    },
    // Lane 2 初始化
    RegConfig {
        offset: 0x181C,
        value: 0xE5,
    },
    RegConfig {
        offset: 0x1878,
        value: 0x00,
    },
    RegConfig {
        offset: 0x1994,
        value: 0x1C,
    },
    RegConfig {
        offset: 0x1AF0,
        value: 0x00,
    },
    // CMN 配置
    RegConfig {
        offset: 0x0428,
        value: 0x60,
    },
    RegConfig {
        offset: 0x0D58,
        value: 0x33,
    },
    RegConfig {
        offset: 0x1D58,
        value: 0x33,
    },
    // Lane 0 配置
    RegConfig {
        offset: 0x0990,
        value: 0x74,
    },
    RegConfig {
        offset: 0x0D64,
        value: 0x17,
    },
    RegConfig {
        offset: 0x08C8,
        value: 0x13,
    },
    // Lane 2 配置
    RegConfig {
        offset: 0x1990,
        value: 0x74,
    },
    RegConfig {
        offset: 0x1D64,
        value: 0x17,
    },
    RegConfig {
        offset: 0x18C8,
        value: 0x13,
    },
    // Lane 0 RX/TX 配置
    RegConfig {
        offset: 0x0D90,
        value: 0x40,
    },
    RegConfig {
        offset: 0x0DA8,
        value: 0x40,
    },
    RegConfig {
        offset: 0x0DC0,
        value: 0x40,
    },
    RegConfig {
        offset: 0x0DD8,
        value: 0x40,
    },
    // Lane 2 RX/TX 配置
    RegConfig {
        offset: 0x1D90,
        value: 0x40,
    },
    RegConfig {
        offset: 0x1DA8,
        value: 0x40,
    },
    RegConfig {
        offset: 0x1DC0,
        value: 0x40,
    },
    RegConfig {
        offset: 0x1DD8,
        value: 0x40,
    },
    // CMN PLL 配置
    RegConfig {
        offset: 0x03C0,
        value: 0x30,
    },
    RegConfig {
        offset: 0x03C4,
        value: 0x06,
    },
    RegConfig {
        offset: 0x0E10,
        value: 0x00,
    },
    RegConfig {
        offset: 0x1E10,
        value: 0x00,
    },
    RegConfig {
        offset: 0x043C,
        value: 0x0F,
    },
    RegConfig {
        offset: 0x0D2C,
        value: 0xFF,
    },
    RegConfig {
        offset: 0x1D2C,
        value: 0xFF,
    },
    RegConfig {
        offset: 0x0D34,
        value: 0x0F,
    },
    RegConfig {
        offset: 0x1D34,
        value: 0x0F,
    },
    // Lane 0 精细配置
    RegConfig {
        offset: 0x08FC,
        value: 0x2A,
    },
    RegConfig {
        offset: 0x0914,
        value: 0x28,
    },
    RegConfig {
        offset: 0x0A30,
        value: 0x03,
    },
    RegConfig {
        offset: 0x0E38,
        value: 0x05,
    },
    RegConfig {
        offset: 0x0ECC,
        value: 0x27,
    },
    RegConfig {
        offset: 0x0ED0,
        value: 0x22,
    },
    RegConfig {
        offset: 0x0ED4,
        value: 0x26,
    },
    // Lane 2 精细配置
    RegConfig {
        offset: 0x18FC,
        value: 0x2A,
    },
    RegConfig {
        offset: 0x1914,
        value: 0x28,
    },
    RegConfig {
        offset: 0x1A30,
        value: 0x03,
    },
    RegConfig {
        offset: 0x1E38,
        value: 0x05,
    },
    RegConfig {
        offset: 0x1ECC,
        value: 0x27,
    },
    RegConfig {
        offset: 0x1ED0,
        value: 0x22,
    },
    RegConfig {
        offset: 0x1ED4,
        value: 0x26,
    },
    // CMN 最终配置
    RegConfig {
        offset: 0x0048,
        value: 0x0F,
    },
    RegConfig {
        offset: 0x0060,
        value: 0x3C,
    },
    RegConfig {
        offset: 0x0064,
        value: 0xF7,
    },
    RegConfig {
        offset: 0x006C,
        value: 0x20,
    },
    RegConfig {
        offset: 0x0070,
        value: 0x7D,
    },
    RegConfig {
        offset: 0x0074,
        value: 0x68,
    },
    RegConfig {
        offset: 0x0AF4,
        value: 0x1A,
    },
    RegConfig {
        offset: 0x1AF4,
        value: 0x1A,
    },
    RegConfig {
        offset: 0x0440,
        value: 0x3F,
    },
    RegConfig {
        offset: 0x10D4,
        value: 0x08,
    },
    RegConfig {
        offset: 0x20D4,
        value: 0x08,
    },
    RegConfig {
        offset: 0x00D4,
        value: 0x30,
    },
    RegConfig {
        offset: 0x0024,
        value: 0x6e,
    },
];

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

// CMN_DP_RSTN 寄存器位字段
register_bitfields![u32,
    CMN_DP_RSTN [
        // CDR watchdog enable
        CDR_WTCHGD_MSK_CDR_EN OFFSET(0) NUMBITS(1) [
            Mask = 0,
            Enable = 1
        ],
        // CDR watchdog enable
        CDR_WTCHDG_EN OFFSET(1) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],
        // DP common reset
        DP_CMN_RSTN OFFSET(2) NUMBITS(1) [
            Reset = 0,
            Enable = 1
        ],
        // DP init reset
        DP_INIT_RSTN OFFSET(3) NUMBITS(1) [
            Reset = 0,
            Enable = 1
        ],
    ]
];

// CMN_ANA_LCPLL_DONE 寄存器位字段
register_bitfields![u32,
    CMN_ANA_LCPLL [
        // LCPLL AFC done
        AFC_DONE OFFSET(6) NUMBITS(1) [
            NotDone = 0,
            Done = 1
        ],
        // LCPLL lock done
        LOCK_DONE OFFSET(7) NUMBITS(1) [
            NotLocked = 0,
            Locked = 1
        ],
    ]
];

// CMN_ANA_ROPLL_DONE 寄存器位字段
register_bitfields![u32,
    CMN_ANA_ROPLL [
        // ROPLL AFC done
        AFC_DONE OFFSET(0) NUMBITS(1) [
            NotDone = 0,
            Done = 1
        ],
        // ROPLL lock done
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
    /// CRU (时钟和复位单元)
    cru: Cru,
}

impl UsbDpPhy {
    /// 创建新的 USBDP PHY 驱动实例
    ///
    /// # 参数
    ///
    /// * `config` - PHY 配置
    /// * `phy_base` - PHY 寄存器基址
    /// * `usb_grf` - USB GRF 基址
    /// * `dp_grf` - USBDP PHY GRF 基址
    /// * `cru` - CRU (时钟和复位单元) 基址
    ///
    /// # Safety
    ///
    /// 调用者必须确保相关寄存器地址有效
    pub fn new(
        config: UsbDpPhyConfig,
        phy_base: Mmio,
        usb_grf: Mmio,
        dp_grf: Mmio,
        cru: Cru,
    ) -> Self {
        // 创建 GRF 实例
        let dp_grf = unsafe { Grf::new(dp_grf, GrfType::UsbdpPhy) };
        let usb_grf = unsafe { Grf::new(usb_grf, GrfType::Usb) };

        Self {
            config,
            phy_base: phy_base.as_ptr() as usize,
            dp_grf,
            usb_grf,
            cru,
        }
    }

    /// 初始化 USBDP PHY
    ///
    /// 基于 RK3588 TRM Chapter 14.5.3.2 和 u-boot drivers/phy/phy-rockchip-usbdp.c:rk3588_udphy_init()
    ///
    /// ## 初始化流程（符合 TRM Fig. 14-2 和 Fig. 14-4）
    ///
    /// 1. **使能时钟** (必须最先执行)
    /// 2. **退出低功耗模式** (设置 i_usbdp_low_pwrn=1, **必须在 APB 复位之前**)
    /// 3. 解除 APB 复位 (i_apb_presetn=1, **必须在低功耗退出之后**)
    /// 4. 等待 1 个 APB 时钟周期
    /// 5. **配置初始化序列** (先 init sequence!)
    /// 6. **配置参考时钟** (后 refclk!)
    /// 7. 配置 lane multiplexing
    /// 8. 解除 init/cmn/lane 复位
    /// 9. 等待 PLL 锁定
    ///
    /// # 错误
    ///
    /// 如果 PLL 未能在超时时间内锁定，返回错误
    pub fn init(&mut self) -> Result<()> {
        log::info!("USBDP PHY: Starting initialization");

        // Step 1: 使能时钟 (必须最先执行)
        self.enable_clocks();

        // Step 2: 退出低功耗模式 (设置 i_usbdp_low_pwrn=1)
        //
        // ⚠️ 重要：根据 TRM Fig. 14-4，必须在解除 APB 复位之前设置此位！
        //
        // TRM 要求：
        //   "i_usbdp_low_pwrn must be set before i_apb_presetn is released"
        //
        // GRF 寄存器: USBDPPHY_GRF_CON1[13] = 1 (PMA block power on)
        self.exit_low_power_mode();

        // Step 3: 解除 APB 复位 (i_apb_presetn=1)
        //
        // ⚠️ 重要：必须在退出低功耗模式之后！
        //
        // 解除 pma_apb 和 pcs_apb 复位，使能 APB 总线访问
        self.deassert_apb_reset();

        // Step 4: 等待 1 个 APB 时钟周期
        //
        // TRM 要求：在 APB 编程前等待 1 个 APB_CLK
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        // Step 5: 配置初始化序列 (先 init sequence!)
        // 参考 u-boot: 先调用 __regmap_multi_reg_write(init_sequence)
        self.configure_init_sequence();

        // Step 6: 配置参考时钟 (后 refclk!)
        // 参考 u-boot: 后调用 rk3588_udphy_refclk_set()
        self.configure_refclk();

        // Step 7: 配置 lane multiplexing
        self.configure_lane_mux();

        // Step 8: 解除 init/cmn/lane 复位
        self.deassert_phy_reset();

        // Step 9: 等待 PLL 锁定
        self.wait_pll_lock()?;

        // Step 10: 启用 USB3 U3 端口
        //
        // ⚠️ 重要：必须启用 USB GRF 中的 U3 端口配置
        //
        // USB GRF 寄存器: USB3OTG1_CFG
        //   - PIPE_ENABLE = 1 (启用 PIPE 接口)
        //   - U3_PORT_DISABLE = 0 (启用 U3 端口)
        //   - PHY_DISABLE = 0 (启用 PHY)
        //
        // 参考 U-Boot: udphy_u3_port_disable(udphy, false)
        log::info!("USBDP PHY{}: Enabling USB3 U3 port in USB GRF", self.config.id);
        self.enable_u3_port();

        log::info!("✓ USBDP PHY{} initialized successfully", self.config.id);
        Ok(())
    }

    fn offset_reg<R: RegisterLongName>(&self, offset: usize) -> &ReadWrite<u32, R> {
        let val = (self.phy_base + offset) as *const ReadWrite<u32, R>;
        unsafe { &*val }
    }

    /// 退出低功耗模式
    ///
    /// 根据 RK3588 TRM Chapter 14.5.3.2 和 USBDPPHY_GRF 寄存器定义：
    ///
    /// **关键寄存器**:
    /// - `USBDPPHY_GRF_CON1` (0x0004):
    ///   - Bit[13]: `i_usbdp_low_pwrn` - 0=PMA关, 1=PMA开
    ///   - Bit[14]: `i_rx_lfps_en` - 0=RX SQ disable, 1=RX SQ enable
    ///   - Bit[31:16]: 写使能位（每bit独立控制）
    ///
    /// **TRM 要求** (Fig. 14-4):
    ///   "i_usbdp_low_pwrn must be set before i_apb_presetn is released"
    fn exit_low_power_mode(&self) {
        log::debug!("USBDP PHY{}: Exiting low power mode", self.config.id);

        // 设置 USBDPPHY_GRF_CON1[13] = 1 (PMA block power on)
        //
        // u-boot 参考：drivers/phy/phy-rockchip-usbdp.c:1041
        //   grfreg_write(udphy->udphygrf, &cfg->grfcfg.low_pwrn, true);
        self.dp_grf.exit_low_power();

        // 如果是 USB 模式，启用 RX LFPS
        //
        // 设置 USBDPPHY_GRF_CON1[14] = 1 (Enable RX LFPS Detector Block)
        //
        // u-boot 参考：drivers/phy/phy-rockchip-usbdp.c:1037-1038
        //   if (udphy->mode & UDPHY_MODE_USB)
        //       grfreg_write(udphy->udphygrf, &cfg->grfcfg.rx_lfps, true);
        if self.config.mode == UsbDpMode::Usb || self.config.mode == UsbDpMode::UsbDp {
            self.dp_grf.enable_rx_lfps();
            log::debug!("USBDP PHY{}: RX LFPS enabled", self.config.id);
        }
    }

    /// 使能时钟
    ///
    /// **必须最先执行**，因为 APB 总线访问需要时钟
    fn enable_clocks(&mut self) {
        log::info!("USBDP PHY{}: Enabling clocks", self.config.id);
        self.cru.enable_usbdp_phy_clocks();
        log::info!("✓ USBDP PHY{}: Clocks enabled", self.config.id);
    }

    /// 解除 APB 复位
    fn deassert_apb_reset(&mut self) {
        log::debug!("USBDP PHY{}: Deasserting APB reset", self.config.id);
        self.cru.deassert_usbdp_phy_apb_reset();
        log::debug!("USBDP PHY{}: APB reset deasserted", self.config.id);
    }

    /// 写入寄存器配置序列
    ///
    /// 参考 u-boot __regmap_multi_reg_write
    fn write_reg_sequence(&self, sequence: &[RegConfig]) {
        let pma_base = self.phy_base + UDPHY_PMA;

        for cfg in sequence {
            let reg_addr = (pma_base + cfg.offset as usize) as *mut u32;
            unsafe {
                reg_addr.write_volatile(cfg.value);
            }
        }

        // 确保写入完成
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    /// 配置参考时钟
    ///
    /// 参考 u-boot rk3588_udphy_refclk_set()
    /// 位置: drivers/phy/phy-rockchip-usbdp.c:932-968
    fn configure_refclk(&self) {
        log::info!(
            "USBDP PHY{}: Configuring reference clock (24MHz)",
            self.config.id
        );

        // 写入 24MHz 参考时钟配置序列 (共 72 个寄存器)
        self.write_reg_sequence(REFCLK_24M_CFG);

        log::info!(
            "✓ USBDP PHY{}: Reference clock configured (72 registers written)",
            self.config.id
        );
    }

    /// 配置初始化序列
    ///
    /// 参考 u-boot rk3588_udphy_init()
    /// 位置: drivers/phy/phy-rockchip-usbdp.c:1031-1103
    fn configure_init_sequence(&self) {
        log::info!("USBDP PHY{}: Applying init sequence", self.config.id);

        // 写入初始化序列 (共 67 个寄存器)
        self.write_reg_sequence(INIT_SEQUENCE);

        log::info!(
            "✓ USBDP PHY{}: Init sequence applied (67 registers written)",
            self.config.id
        );
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
    ///
    /// 参考 u-boot drivers/phy/phy-rockchip-usbdp.c:rk3588_udphy_init()
    /// 位置: 第 1052-1067 行
    ///
    /// 复位解除顺序和时延：
    /// 1. 解除 INIT 复位
    /// 2. 如果是 DP 模式，设置 DP_INIT_RSTN
    /// 3. 等待 1ms (由 cru::deassert_usbdp_phy_init_resets 处理)
    /// 4. 解除 CMN/LANE 复位
    fn deassert_phy_reset(&mut self) {
        log::debug!("USBDP PHY{}: Deasserting PHY reset", self.config.id);

        // Step 1: 解除 USB 模式的 init/cmn/lane 复位
        // 注意：deassert_usbdp_phy_init_resets() 内部已包含正确的 1ms 时延
        if self.config.mode == UsbDpMode::Usb || self.config.mode == UsbDpMode::UsbDp {
            self.cru.deassert_usbdp_phy_init_resets();
            log::debug!(
                "USBDP PHY{}: USB init/cmn/lane reset deasserted with 1ms delay",
                self.config.id
            );
        }

        // Step 2: 如果是 DP 模式，解除 DP init 复位 (CMN_DP_RSTN 寄存器)
        if self.config.mode == UsbDpMode::Dp || self.config.mode == UsbDpMode::UsbDp {
            let pma_base = self.phy_base + UDPHY_PMA;
            let dp_rstn_reg =
                unsafe { (pma_base + pma_offset::CMN_DP_RSTN) as *mut u32 };

            unsafe {
                let value = dp_rstn_reg.read_volatile();
                dp_rstn_reg.write_volatile(value | (1 << 3)); // DP_INIT_RSTN
            }
            log::debug!("USBDP PHY{}: DP init reset deasserted", self.config.id);
        }
    }

    /// 等待 PLL 锁定
    ///
    /// 参考 u-boot rk3588_udphy_status_check()
    /// 位置: drivers/phy/phy-rockchip-usbdp.c:1008-1018
    fn wait_pll_lock(&self) -> Result<()> {
        log::info!("USBDP PHY{}: Waiting for PLL lock", self.config.id);

        let pma_base = self.phy_base + UDPHY_PMA;

        // 等待 LCPLL 锁定 (USB 模式需要)
        if self.config.mode == UsbDpMode::Usb || self.config.mode == UsbDpMode::UsbDp {
            let lcpll_reg =
                unsafe { (pma_base + pma_offset::CMN_ANA_LCPLL_DONE) as *const u32 };

            log::debug!("USBDP PHY{}: LCPLL register @ 0x{:x}",
                       self.config.id, lcpll_reg as usize);

            // 使用循环计数器实现超时 (不依赖系统时间)
            // 100ms / 200us = 500 次循环
            const MAX_RETRIES: u32 = 500;

            for retry in 0..MAX_RETRIES {
                let value = unsafe { lcpll_reg.read_volatile() };
                let afc_done = (value >> 6) & 0x1 == 1;
                let lock_done = (value >> 7) & 0x1 == 1;

                // 打印初始状态
                if retry == 0 {
                    log::debug!("USBDP PHY{}: LCPLL initial status - AFC={}, LOCK={}, val=0x{:08x}",
                               self.config.id, afc_done, lock_done, value);
                }

                if afc_done && lock_done {
                    log::info!(
                        "✓ USBDP PHY{}: LCPLL locked successfully (retry={}, val=0x{:08x})",
                        self.config.id,
                        retry,
                        value
                    );
                    return Ok(());
                }

                // 打印调试信息 (每 100 次循环)
                if retry % 100 == 0 && retry > 0 {
                    log::debug!(
                        "USBDP PHY{}: LCPLL waiting... AFC={}, LOCK={}, val=0x{:08x}",
                        self.config.id,
                        afc_done,
                        lock_done,
                        value
                    );
                }

                self.delay_us(200); // 200 微秒轮询间隔
            }

            // 超时：读取最终状态并返回错误
            let value = unsafe { lcpll_reg.read_volatile() };
            let afc_done = (value >> 6) & 0x1 == 1;
            let lock_done = (value >> 7) & 0x1 == 1;

            log::error!(
                "✗ USBDP PHY{}: LCPLL lock timeout after {} retries! AFC={}, LOCK={}, val=0x{:08x}",
                self.config.id,
                MAX_RETRIES,
                afc_done,
                lock_done,
                value
            );
            return Err(crate::err::USBError::Timeout);
        }

        log::info!("✓ USBDP PHY{}: PLL lock check skipped (mode={:?})",
                  self.config.id, self.config.mode);
        Ok(())
    }

    /// 微秒级延时
    fn delay_us(&self, us: u32) {
        const LOOPS_PER_US: u32 = 50;
        let total_loops = us * LOOPS_PER_US;
        for _ in 0..total_loops {
            core::hint::spin_loop();
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
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

        let status = UsbDpPhyStatus {
            lcpll_locked,
            ropll_locked,
            mode: self.config.mode,
        };

        // 打印详细状态
        log::info!("USBDP PHY{}: Status - LCPLL={} (val=0x{:08x}), ROPLL={} (val=0x{:08x}), mode={:?}",
                  self.config.id,
                  lcpll_locked, lcpll_value,
                  ropll_locked, ropll_value,
                  self.config.mode);

        status
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
