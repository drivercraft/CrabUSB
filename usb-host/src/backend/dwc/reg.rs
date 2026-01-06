//! DWC3 寄存器定义
//! 基于 Linux drivers/usb/dwc3/core.h

use core::hint::spin_loop;
use core::sync::atomic::Ordering;

use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};
use tock_registers::{
    register_bitfields,
    registers::{ReadOnly, ReadWrite},
};

use crate::osal::SpinWhile;

use super::consts::*;

/// DWC3 全局寄存器基址偏移 (相对于 xHCI 寄存器区域)
const DWC3_GLOBALS_REGS_START: usize = 0xc100;

pub struct Dwc3Hwparams {
    pub hwparams0: u32,
    pub hwparams1: u32,
    pub hwparams2: u32,
    pub hwparams3: u32,
    pub hwparams4: u32,
    pub hwparams5: u32,
    pub hwparams6: u32,
    pub hwparams7: u32,
    pub hwparams8: u32,
}

/// DWC3 寄存器映射结构
#[repr(C)]
pub struct Dwc3Registers {
    // 0xc100 - 0xc10c: 保留和其他寄存器
    _reserved0: [u32; 4],

    /// 0xc110 - Global Control Register
    pub gctl: ReadWrite<u32, GCTL::Register>,

    /// 0xc114 - Global Event Enable Register
    pub gevten: ReadWrite<u32, GEVTEN::Register>,

    /// 0xc118 - Global Status Register
    pub gsts: ReadWrite<u32, GSTS::Register>,

    /// 0xc11c - Global User Control 1 Register
    pub guctl1: ReadWrite<u32, GUCTL1::Register>,

    /// 0xc120 - SNPSID Register (只读)
    pub gsnpsid: ReadOnly<u32, GSNPSID::Register>,

    /// 0xc124 - GPIO Register
    pub ggpio: ReadWrite<u32, GGPIO::Register>,

    /// 0xc128 - GUID Register
    pub guid: ReadWrite<u32, GUID::Register>,

    /// 0xc12c - User Control Register
    pub guctl: ReadWrite<u32, GUCTL::Register>,

    // 0xc130 - 0xc1fc: 其他寄存器
    _reserved1: [u32; 46],

    /// 0xc200 - USB2 PHY Configuration Register 0
    pub gusb2phycfg0: ReadWrite<u32, GUSB2PHYCFG::Register>,

    // 0xc204 - 0xc2bc: 保留
    _reserved2: [u32; 28],

    /// 0xc2c0 - USB3 PIPE Control Register 0
    pub gusb3pipectl0: ReadWrite<u32, GUSB3PIPECTL::Register>,

    // 0xc2c4 - 0xc700: 保留
    _reserved3: [u32; 273],

    /// 0xc704 - Device Control Register
    pub dctl: ReadWrite<u32, DCTL::Register>,
}

// =============================================================================
// 寄存器位字段定义
// =============================================================================

// Global Control Register (GCTL) - 0xc110
register_bitfields![u32,
    GCTL [
        /// 禁止时钟门控
        DSBLCLKGTNG OFFSET(0) NUMBITS(1) [
            Enable = 0,
            Disable = 1
        ],

        /// 全局休眠使能
        GBLHIBERNATIONEN OFFSET(1) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// U2 退出 LFPS
        U2EXIT_LFPS OFFSET(2) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 禁止 scrambler
        DISSCRAMBLE OFFSET(3) NUMBITS(1) [
            Enable = 0,
            Disable = 1
        ],

        /// 缩放因子
        SCALEDOWN OFFSET(4) NUMBITS(2) [
            None = 0,
            Minimum = 1,
            Low = 2,
            Maximum = 3
        ],

        /// 时钟选择
        RAMCLKSEL OFFSET(6) NUMBITS(2) [
            Bus = 0,
            Pipe = 1,
            PipeHalf = 2
        ],

        /// 帧内同步
        SOFITPSYNC OFFSET(10) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 核心软复位
        CORESOFTRESET OFFSET(11) NUMBITS(1) [
            Normal = 0,
            Reset = 1
        ],

        /// 端口能力方向
        PRTCAPDIR OFFSET(12) NUMBITS(2) [
            Host = 1,
            Device = 2,
            OTG = 3
        ],

        /// U2 复位使能控制
        U2RSTECN OFFSET(16) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 电源 down 缩放因子
        PWRDNSCALE OFFSET(19) NUMBITS(13) []
    ]
];

/// Global Status Register (GSTS) - 0xc118
register_bitfields![u32,
    GSTS [
        /// 当前模式
        CURMOD OFFSET(0) NUMBITS(2) [
            Device = 0,
            Host = 1
        ],

        /// 总线错误地址有效
        BUS_ERR_ADDR_VLD OFFSET(4) NUMBITS(1) [],

        /// CSR 超时
        CSR_TIMEOUT OFFSET(5) NUMBITS(1) [],

        /// 设备 IP 处理中
        DEVICE_IP OFFSET(6) NUMBITS(1) [],

        /// 主机 IP 处理中
        HOST_IP OFFSET(7) NUMBITS(1) []
    ]
];

// SNPSID Register (GSNPSID) - 0xc120 (只读)
register_bitfields![u32,
    GSNPSID [
        /// 仿真 ID
        SIMULATION OFFSET(31) NUMBITS(1) [
            Production = 0,
            Simulation = 1
        ],

        /// 修订号
        REVISION OFFSET(16) NUMBITS(16) [],

        /// 产品 ID
        PRODUCT_ID OFFSET(0) NUMBITS(16) []
    ]
];

/// Global USB2 PHY Configuration Register (GUSB2PHYCFG) - 0xc200
register_bitfields![u32,
    GUSB2PHYCFG [
        /// PHY 软复位
        PHYSOFTRST OFFSET(31) NUMBITS(1) [
            Normal = 0,
            Reset = 1
        ],

        /// 使能低功耗暂停
        ENBLSLPM OFFSET(29) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 自由时钟存在
        U2_FREECLK_EXISTS OFFSET(30) NUMBITS(1) [
            No = 0,
            Yes = 1
        ],

        /// USB 转发时间
        USBTRDTIM OFFSET(10) NUMBITS(4) [],

        /// 使能低功耗暂停
        SUSPHY OFFSET(6) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// ULPI 或 UTMI+
        ULPI_UTMI OFFSET(4) NUMBITS(1) [
            UTMI = 0,
            ULPI = 1
        ],

        /// PHY 接口
        PHYIF OFFSET(3) NUMBITS(1) [
            EightBit = 0,
            SixteenBit = 1
        ]
    ]
];

/// Global USB3 PIPE Control Register (GUSB3PIPECTL) - 0xc2c0
register_bitfields![u32,
    GUSB3PIPECTL [
        /// PIPE 物理复位
        PHYSOFTRST OFFSET(31) NUMBITS(1) [
            Normal = 0,
            Reset = 1
        ],

        /// 使能 LBP
        ENABLE_LBP OFFSET(30) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 发送延迟
        TX_DEEMPHASIS OFFSET(29) NUMBITS(1) [
            Minus6dB = 0,
            Minus3_5dB = 1
        ],

        /// 暂停时 PIPE 进入 P3
        PIPE_P3_P2_TO_P1 OFFSET(25) NUMBITS(1) [
            No = 0,
            Yes = 1
        ],

        /// 在 U0 中请求暂停
        REQP0P1P2P3 OFFSET(24) NUMBITS(1) [
            No = 0,
            Yes = 1
        ],

        /// 实现延迟
        U1U2_EXIT_LATENCY OFFSET(17) NUMBITS(1) [
            No = 0,
            Yes = 1
        ],

        /// PHY 配置
        PHY_CONFIG OFFSET(16) NUMBITS(1) [
            Unchanged = 0,
            Force = 1
        ],

        /// U2 状态进入 P3
        U2SSINP3OK OFFSET(15) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 禁止接收检测在 P3
        DISRXDETINP3 OFFSET(14) NUMBITS(1) [
            Enable = 0,
            Disable = 1
        ],

        /// TX 历史
        TX_TX_HISTORY_T OFFSET(6) NUMBITS(1) [
            FullSpeed = 0,
            HighSpeed = 1
        ],

        /// 实体延迟发送
        LATENCY_OFFSET_TX OFFSET(3) NUMBITS(1) [
            No = 0,
            Yes = 1
        ],

        /// 实体延迟接收
        LATENCY_OFFSET_RX OFFSET(2) NUMBITS(1) [
            No = 0,
            Yes = 1
        ],

        /// 使能暂停 PHY
        SUSPHY OFFSET(1) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 延迟 P1/P2 到 P0
        UX_EXIT_PX OFFSET(0) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ]
    ]
];

/// Global Event Enable Register (GEVTEN) - 0xc114
register_bitfields![u32,
    GEVTEN [
        /// OTG 事件使能
        OTGEVTEN OFFSET(17) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 设备事件使能
        DEVEVTEN OFFSET(16) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// Carkit 事件使能
        CARKITEVTEN OFFSET(8) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// I2C 事件使能
        I2CEVTEN OFFSET(7) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ]
    ]
];

/// Global User Control 1 Register (GUCTL1) - 0xc11c
register_bitfields![u32,
    GUCTL1 [
        /// 设备解耦 L1L2 事件
        DEV_DECOUPLE_L1L2_EVT OFFSET(31) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// TX IPGAP 线检查禁止
        TX_IPGAP_LINECHECK_DIS OFFSET(28) NUMBITS(1) [
            Enable = 0,
            Disable = 1
        ],

        /// 驱动强制 20_CLK 用于 30_CLK
        DEV_FORCE_20_CLK_FOR_30_CLK OFFSET(26) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 设备 L1 退出由硬件
        DEV_L1_EXIT_BY_HW OFFSET(24) NUMBITS(1) [
            No = 0,
            Yes = 1
        ],

        /// 禁止 SS 停车模式
        PARKMODE_DISABLE_SS OFFSET(17) NUMBITS(1) [
            Enable = 0,
            Disable = 1
        ],

        /// 恢复操作模式 HS 主机
        RESUME_OPMODE_HS_HOST OFFSET(10) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ]
    ]
];

/// User Control Register (GUCTL) - 0xc12c
register_bitfields![u32,
    GUCTL [
        /// 跳止发送
        GTSTOP_SEND OFFSET(31) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 禁止 HNP
        HST_DISCONNECT OFFSET(17) NUMBITS(1) [
            Enable = 0,
            Disable = 1
        ],

        /// 触发 USB 链接
        USBTRGTIM OFFSET(10) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ]
    ]
];

/// GPIO Register (GGPIO) - 0xc124
register_bitfields![u32,
    GGPIO [
        /// GPIO 方向
        GPIO_DIR OFFSET(16) NUMBITS(16) [],

        /// GPIO 数据
        GPIO_DATA OFFSET(0) NUMBITS(16) []
    ]
];

/// GUID Register (GUID) - 0xc128
register_bitfields![u32,
    GUID [
        /// GUID 值
        GUID_VALUE OFFSET(0) NUMBITS(32) []
    ]
];

/// Device Control Register (DCTL) - 0xc704
register_bitfields![u32,
    DCTL [
        /// 运行/停止 (bit 31)
        /// 0 = 停止，1 = 运行
        RUN_STOP OFFSET(31) NUMBITS(1) [
            Stop = 0,
            Run = 1
        ],

        /// 核心软复位 (bit 30)
        CSFTRST OFFSET(30) NUMBITS(1) [
            Normal = 0,
            Reset = 1
        ],

        /// 链路层软复位 (bit 29)
        LSFTRST OFFSET(29) NUMBITS(1) [
            Normal = 0,
            Reset = 1
        ],

        /// HIRD 阈值 (bits 24-28)
        /// 主机发起的远程唤醒延迟阈值
        HIRD_THRES OFFSET(24) NUMBITS(5) [],

        /// 应用层特定复位 (bit 23)
        APPL1RES OFFSET(23) NUMBITS(1) [
            Normal = 0,
            Reset = 1
        ],

        /// LPM Errata (bits 20-23)
        /// 仅适用于版本 1.94a 及更新
        LPM_ERRATA OFFSET(20) NUMBITS(4) [],

        /// 保持连接 (bit 19)
        /// 仅适用于版本 1.94a 及更新
        KEEP_CONNECT OFFSET(19) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// L1 休眠使能 (bit 18)
        L1_HIBER_EN OFFSET(18) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 继续远程唤醒 (bit 17)
        CRS OFFSET(17) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 继续同步 (bit 16)
        CSS OFFSET(16) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 初始化 U2 使能 (bit 12)
        INITU2ENA OFFSET(12) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 接受 U2 使能 (bit 11)
        ACCEPTU2ENA OFFSET(11) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 初始化 U1 使能 (bit 10)
        INITU1ENA OFFSET(10) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 接受 U1 使能 (bit 9)
        ACCEPTU1ENA OFFSET(9) NUMBITS(1) [
            Disable = 0,
            Enable = 1
        ],

        /// 测试控制掩码 (bits 1-4)
        TSTCTRL_MASK OFFSET(1) NUMBITS(4) [],

        /// USB 链路状态改变请求 (bits 5-8)
        ULSTCHNGREQ OFFSET(5) NUMBITS(4) [
            NoAction = 0,
            SSDisabled = 4,
            RxDetect = 5,
            SSInactive = 6,
            Recovery = 8,
            Compliance = 10,
            Loopback = 11
        ]
    ]
];

/// DWC3 寄存器访问器
pub struct Dwc3Regs {
    base: usize,
}

impl Dwc3Regs {
    /// 创建新的 DWC3 寄存器访问器
    ///
    /// # Safety
    ///
    /// 调用者必须确保 `base` 地址有效且可以访问
    pub unsafe fn new(base: usize) -> Self {
        Self { base }
    }

    /// 获取全局寄存器
    pub fn globals(&self) -> &'static Dwc3Registers {
        let addr = self.base + DWC3_GLOBALS_REGS_START;
        unsafe { &*(addr as *const Dwc3Registers) }
    }

    fn reg_offset(&self, offset: usize) -> *mut u32 {
        (self.base + DWC3_GLOBALS_REGS_START + offset) as *mut u32
    }

    /// 获取可变的全局寄存器
    fn globals_mut(&mut self) -> &'static mut Dwc3Registers {
        let addr = self.base + DWC3_GLOBALS_REGS_START;
        unsafe { &mut *(addr as *mut Dwc3Registers) }
    }

    // ==================== 寄存器操作封装 ====================

    // pub fn hwparams(&self) -> Dwc3Hwparams {
    //     Dwc3Hwparams {
    //         hwparams0: self.globals().gsnpsid.get(),
    //         hwparams1: 0, // TODO: 读取其他 HWPARAMS 寄存器
    //         hwparams2: 0,
    //         hwparams3: 0,
    //         hwparams4: 0,
    //         hwparams5: 0,
    //         hwparams6: 0,
    //         hwparams7: 0,
    //         hwparams8: 0,
    //     }
    // }

    /// 读取 SNPSID 的产品 ID
    pub fn read_product_id(&self) -> u16 {
        self.globals().gsnpsid.read(GSNPSID::PRODUCT_ID) as u16
    }

    /// 读取 SNPSID 的版本号
    pub fn read_revision(&self) -> u32 {
        self.globals().gsnpsid.read(GSNPSID::REVISION) << 16
    }

    pub async fn device_soft_reset(&mut self) {
        self.globals().dctl.modify(DCTL::CSFTRST::Reset);
        trace!("DWC3: Device waiting for soft reset...");
        SpinWhile::new(|| self.globals().dctl.is_set(DCTL::CSFTRST)).await;
        trace!("DWC3: Device soft reset completed");
    }

    pub async fn core_soft_reset(&mut self) {
        // Before Resetting PHY, put Core in Reset
        self.globals().gctl.modify(GCTL::CORESOFTRESET::Reset);

        // Assert USB3 PHY reset
        self.globals()
            .gusb3pipectl0
            .modify(GUSB3PIPECTL::PHYSOFTRST::Reset);

        self.globals()
            .gusb2phycfg0
            .modify(GUSB2PHYCFG::PHYSOFTRST::Reset);

        self.delay_ms(100);

        // Clear USB3 PHY reset
        self.globals()
            .gusb3pipectl0
            .modify(GUSB3PIPECTL::PHYSOFTRST::Normal);

        // Clear USB2 PHY reset
        self.globals()
            .gusb2phycfg0
            .modify(GUSB2PHYCFG::PHYSOFTRST::Normal);

        self.delay_ms(100);

        // After PHYs are stable we can take Core out of reset state
        self.globals().gctl.modify(GCTL::CORESOFTRESET::Normal);
    }

    /// 修改 GCTL 的 PRTCAPDIR 字段
    pub fn set_prtcap_dir(&mut self, mode: u32) {
        self.globals_mut().gctl.modify(GCTL::PRTCAPDIR.val(mode));
    }

    /// 读取 GSTS.CURMOD（当前模式）
    pub fn read_current_mode(&self) -> u32 {
        self.globals().gsts.read(GSTS::CURMOD)
    }

    /// 读取 GUSB2PHYCFG 寄存器
    pub fn read_gusb2phy_cfg(&self) -> u32 {
        self.globals().gusb2phycfg0.get()
    }

    /// 设置 GUSB2PHYCFG 寄存器
    pub fn write_gusb2phy_cfg(&mut self, value: u32) {
        self.globals_mut().gusb2phycfg0.set(value);
        // 内存屏障，确保写入完成
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    /// 读取 GUSB3PIPECTL 寄存器
    pub fn read_gusb3pipe_ctl(&self) -> u32 {
        self.globals().gusb3pipectl0.get()
    }

    /// 设置 GUSB3PIPECTL 寄存器
    pub fn write_gusb3pipe_ctl(&mut self, value: u32) {
        self.globals_mut().gusb3pipectl0.set(value);
        // 内存屏障，确保写入完成
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    /// USB3 PHY 软复位
    pub fn usb3_phy_reset(&mut self, assert: bool) {
        let reg = self.read_gusb3pipe_ctl();
        if assert {
            self.write_gusb3pipe_ctl(reg | (1 << 31));
        } else {
            self.write_gusb3pipe_ctl(reg & !(1 << 31));
        }
    }

    /// USB2 PHY 软复位
    pub fn usb2_phy_reset(&mut self, assert: bool) {
        let reg = self.read_gusb2phy_cfg();
        if assert {
            self.write_gusb2phy_cfg(reg | (1 << 31));
        } else {
            self.write_gusb2phy_cfg(reg & !(1 << 31));
        }
    }

    /// 配置 USB3 PHY
    pub fn configure_usb3_phy(&mut self) {
        log::debug!("DWC3: configure_usb3_phy() called");

        // 使用 modify 方法直接修改寄存器位字段
        // 设置 U2SSINP3OK=1 (bit 15)
        log::debug!("DWC3: Setting U2SSINP3OK=1 using modify()");
        self.globals_mut()
            .gusb3pipectl0
            .modify(GUSB3PIPECTL::U2SSINP3OK.val(1));

        log::debug!("DWC3: modify() completed");
    }

    /// 配置 USB2 PHY
    pub fn configure_usb2_phy(&mut self) {
        // 使用 modify 方法配置多个字段
        self.globals_mut().gusb2phycfg0.modify(
            GUSB2PHYCFG::PHYIF.val(1) +          // 16-bit UTMI
            GUSB2PHYCFG::USBTRDTIM.val(9), // 16-bit UTMI turnaround time
        );
    }

    /// 检查 PHY 是否仍在复位
    pub fn is_phy_in_reset(&self) -> (bool, bool) {
        let gusb2 = self.read_gusb2phy_cfg();
        let gusb3 = self.read_gusb3pipe_ctl();
        let usb2_reset = (gusb2 >> 31) & 0x1 == 1;
        let usb3_reset = (gusb3 >> 31) & 0x1 == 1;
        (usb2_reset, usb3_reset)
    }

    // // ==================== 高级方法封装 ====================

    // /// 验证 SNPSID 寄存器
    // ///
    // /// 返回 (ip_id, product_id, revision)
    // pub fn verify_snpsid(&self) -> (u16, u16, u16) {
    //     let snpsid_full = self.read_snpsid();
    //     let product_id = self.read_product_id();
    //     let revision = self.read_revision();
    //     let ip_id = (snpsid_full >> 16) & 0xffff;

    //     log::info!(
    //         "DWC3 SNPSID: full={:#010x}, ip_id={:#06x}, product_id={:#06x}, revision={:#06x}",
    //         snpsid_full,
    //         ip_id,
    //         product_id,
    //         revision
    //     );

    //     match ip_id {
    //         0x5533 => {
    //             log::info!("Detected DWC_usb3 controller");
    //         }
    //         0x3331 => {
    //             log::info!("Detected DWC_usb31 controller");
    //         }
    //         0x3332 => {
    //             log::info!("Detected DWC_usb32 controller");
    //         }
    //         _ => {
    //             log::warn!(
    //                 "Unknown DWC3 IP ID: {:#06x} (expected 0x5533, 0x3331, or 0x3332)",
    //                 ip_id
    //             );
    //             log::warn!("Continuing with initialization - may be a custom or FPGA variant");
    //         }
    //     }

    //     (ip_id as u16, product_id, revision)
    // }

    pub fn num_event_buffers(&self) -> usize {
        let val = unsafe { self.reg_offset(DWC3_GHWPARAMS1).read_volatile() };
        (((val) & (0x3f << 15)) >> 15) as usize
    }

    /// 配置 GCTL 寄存器
    ///
    /// 参考 Linux drivers/usb/dwc3/core.c:dwc3_core_init()
    pub fn setup_gctl(&mut self) {
        log::info!("DWC3: Configuring GCTL");

        let current = self.globals().gctl.get();
        log::debug!("DWC3: GCTL before configuration: {:#010x}", current);

        let revision = self.read_revision() as u32;

        log::info!("DWC3: SNPSID revision: {:#06x}", revision);

        let mut reg = current;

        // 清除 SCALEDOWN (bits 4-5) - 禁用 scale down
        reg &= !(0x3 << 4);

        // 清除 DISSCRAMBLE (bit 3) - 启用 scrambler
        reg &= !(1 << 3);

        // 对于版本 < 1.90a，设置 U2RSTECN
        if revision < 0x190a {
            reg |= 1 << 16; // U2RSTECN
            log::debug!("DWC3: Enabled U2RSTECN for revision < 1.90a");
        }

        // 尝试启用时钟门控
        reg &= !(1 << 0); // DSBLCLKGTNG = 0 (enable clock gating)
        log::debug!("DWC3: Clock gating enabled");

        self.globals().gctl.set(reg);

        let updated = self.globals().gctl.get();
        log::debug!("DWC3: GCTL after configuration: {:#010x}", updated);

        // 验证关键位
        let prtcapdir = (updated >> 12) & 0x3;
        let scaledown = (updated >> 4) & 0x3;
        let disscramble = (updated >> 3) & 0x1;
        let dsblclkgtng = updated & 0x1;

        log::info!(
            "DWC3: GCTL config - PRTCAPDIR={}, SCALEDOWN={}, DISSCRAMBLE={}, DSBLCLKGTNG={}",
            prtcapdir,
            scaledown,
            disscramble,
            dsblclkgtng
        );

        log::info!("DWC3: GCTL configuration complete");
    }

    /// 设置 DWC3 为 HOST 模式
    ///
    /// 根据 Linux DWC3 驱动的实现，HOST 模式下：
    /// 1. 设置 GCTL.PRTCAPDIR = DWC3_GCTL_PRTCAP_HOST (1)
    pub fn setup_host_mode(&mut self) {
        log::info!("DWC3: Configuring HOST mode");

        let current = self.globals().gctl.get();
        log::debug!("DWC3: Current GCTL: {:#010x}", current);

        let current_prtcap = (current >> 12) & 0x3;
        log::debug!(
            "DWC3: Current PRTCAPDIR: {} (0=Device, 1=Host, 2=Device, 3=OTG)",
            current_prtcap
        );

        // 设置为 HOST 模式
        self.set_prtcap_dir(1); // 1 = Host mode

        let updated = self.globals().gctl.get();
        let updated_prtcap = (updated >> 12) & 0x3;
        log::debug!("DWC3: Updated GCTL: {:#010x}", updated);
        log::debug!("DWC3: Updated PRTCAPDIR: {}", updated_prtcap);

        log::info!(
            "DWC3: Configured in HOST mode (PRTCAPDIR={})",
            updated_prtcap
        );

        // 验证模式切换完成
        let current_mode = self.read_current_mode();
        log::info!(
            "DWC3: Current GSTS.CURMOD: {} (0=Device, 1=Host)",
            current_mode
        );

        if current_mode == 1 {
            log::info!("✓ DWC3 successfully switched to HOST mode");
        } else {
            log::warn!(
                "⚠ DWC3 mode mismatch: expected 1 (Host), got {}",
                current_mode
            );
        }
    }

    /// 配置 PHY
    ///
    /// 配置 USB2 和 USB3 PHY 的基本参数
    ///
    /// **⚠️ RK3588 特殊处理**：
    ///
    /// 在 RK3588 上，DWC3 的 PHY 配置寄存器 (GUSB2PHYCFG, GUSB3PIPECTL) **不可访问**，
    /// 始终返回 0x00000000。这是正常现象！
    ///
    /// 原因：Rockchip 将 PHY 配置完全通过 GRF 寄存器实现：
    /// - USB2 PHY 配置 → USB2PHY GRF (0xfd5d4000)
    /// - USB3 PHY 配置 → USBDP PHY GRF (0xfd5cc000)
    /// - DWC3 PHY 寄存器 → 可能未连接或被桥接
    ///
    /// 因此，此方法在 RK3588 上跳过 DWC3 PHY 寄存器配置，因为：
    /// 1. USB2 PHY 已经通过 usb2phy_grf 正确初始化
    /// 2. USB3 PHY 已经通过 usbdp_phy_grf 正确初始化
    /// 3. PHY 复位和配置已经在 phy.init() 中完成
    ///
    /// 参考 Linux 内核 drivers/usb/dwc3/core.c:dwc3_phy_setup()
    /// 和 u-boot drivers/usb/host/xhci-dwc3.c
    pub fn setup_phy(&mut self) -> core::result::Result<(), usb_if::host::USBError> {
        log::info!("DWC3: Starting PHY configuration");

        // === 步骤 0: 读取并记录初始状态 ===
        let gusb2_init = self.read_gusb2phy_cfg();
        let gusb3_init = self.read_gusb3pipe_ctl();

        log::info!("DWC3: Initial DWC3 PHY register states:");
        log::info!("  GUSB2PHYCFG:   {:#010x}", gusb2_init);
        log::info!("  GUSB3PIPECTL:   {:#010x}", gusb3_init);

        // === 检测是否为 RK3588（PHY 寄存器不可访问）===
        if gusb2_init == 0 && gusb3_init == 0 {
            log::warn!("⚠ DWC3: PHY registers read as 0x00000000");
            log::warn!("⚠ DWC3: This is NORMAL on RK3588!");
            log::info!("ℹ DWC3: RK3588 uses GRF-based PHY configuration:");
            log::info!("   - USB2 PHY configured via USB2PHY GRF (0xfd5d4000)");
            log::info!("   - USB3 PHY configured via USBDP PHY GRF (0xfd5cc000)");
            log::info!("   - DWC3 PHY registers are not accessible (hardware limitation)");
            log::info!("ℹ DWC3: PHY initialization was completed in phy.init()");
            log::info!("✓ DWC3: Skipping DWC3 PHY register configuration (RK3588)");

            // 在 RK3588 上，PHY 配置完全由 GRF 处理，不需要配置 DWC3 PHY 寄存器
            // 直接返回成功
            return Ok(());
        }

        // === 标准 DWC3 初始化流程（非 RK3588 平台）===
        log::info!("DWC3: DWC3 PHY registers accessible, performing standard configuration");

        // === 步骤 1: PHY 复位序列 ===
        log::info!("DWC3: PHY reset sequence");

        // Assert PHY resets
        self.usb3_phy_reset(true);
        self.usb2_phy_reset(true);

        // 延时 100ms
        log::debug!("DWC3: Waiting 100ms for PHY reset");
        self.delay_ms(100);

        // Deassert PHY resets
        self.usb3_phy_reset(false);
        self.usb2_phy_reset(false);

        // 额外延时，确保 PHY 复位完成
        self.delay_ms(50);

        // 验证复位已解除
        let (usb2_reset, usb3_reset) = self.is_phy_in_reset();
        if !usb2_reset && !usb3_reset {
            log::info!("DWC3: PHY reset complete (both PHYs out of reset)");
        } else {
            log::warn!("DWC3: PHY reset may not have completed!");
        }

        // === 步骤 2: 配置 USB3 PHY ===
        log::info!("DWC3: Configuring USB3 PHY");
        let reg_before = self.read_gusb3pipe_ctl();
        log::debug!("DWC3: GUSB3PIPECTL before: {:#010x}", reg_before);

        self.configure_usb3_phy();

        let updated = self.read_gusb3pipe_ctl();
        log::debug!("DWC3: GUSB3PIPECTL after configure: {:#010x}", updated);

        // === 步骤 3: 配置 USB2 PHY ===
        log::info!("DWC3: Configuring USB2 PHY");
        let reg_before = self.read_gusb2phy_cfg();
        log::debug!("DWC3: GUSB2PHYCFG before: {:#010x}", reg_before);

        self.configure_usb2_phy();

        let updated = self.read_gusb2phy_cfg();
        log::debug!("DWC3: GUSB2PHYCFG after:  {:#010x}", updated);

        // 验证关键位
        let phyif = (updated >> 3) & 0x1;
        let usbtrdtim = (updated >> 10) & 0xF;
        let susphy = (updated >> 6) & 0x1;
        let enblslpm = (updated >> 29) & 0x1;
        let physoftrst = (updated >> 31) & 0x1;

        log::info!(
            "DWC3: USB2 PHY config - PHYIF={}, USBTRDTIM={}, SUSPHY={}, ENBLSLPM={}, PHYSOFTRST={}",
            phyif,
            usbtrdtim,
            susphy,
            enblslpm,
            physoftrst
        );

        // === 步骤 4: 等待 PHY 稳定 ===
        log::info!("DWC3: Waiting for PHY to stabilize (50ms)");
        self.delay_ms(50);

        // 再次读取确认配置已生效
        let gusb2_final = self.read_gusb2phy_cfg();
        let gusb3_final = self.read_gusb3pipe_ctl();

        // 验证 PHY 复位已解除
        let (usb2_reset, usb3_reset) = self.is_phy_in_reset();
        if usb2_reset {
            log::error!("DWC3: USB2 PHY still in reset!");
        }
        if usb3_reset {
            log::error!("DWC3: USB3 PHY still in reset!");
        }

        // === 验证 PHY 寄存器是否可写 ===
        log::info!("DWC3: Final PHY register states:");
        log::info!("DWC3:   GUSB2PHYCFG:   {:#010x}", gusb2_final);
        log::info!("DWC3:   GUSB3PIPECTL:   {:#010x}", gusb3_final);

        // 检查 PHY 寄存器是否仍为 0（无法写入）
        if gusb2_final == 0 && gusb3_final == 0 {
            log::error!("❌ DWC3: PHY registers are still 0x00000000 after configuration!");
            log::error!("❌ DWC3: This indicates PHY is not accessible or clocks are not enabled");
            log::error!("❌ DWC3: Possible root causes:");
            log::error!("   1. USB2 PHY UTMI 480MHz clock not running");
            log::error!("   2. USBDP PHY PIPE interface not initialized");
            log::error!("   3. DWC3 controller clock domain not active");
            log::error!("   4. PHY hardware not properly powered on");
            log::error!("   5. RK3588 hardware limitation (see above)");

            // 打印诊断信息
            log::error!("❌ DWC3: Diagnostic information:");
            log::error!("   GCTL:          {:#010x}", self.globals().gctl.get());
            log::error!("   GSTS:          {:#010x}", self.globals().gsts.get());
            log::error!("   GGPIO:         {:#010x}", self.globals().ggpio.get());

            return Err(usb_if::host::USBError::NotInitialized);
        }

        log::info!("✓ DWC3: PHY configuration complete (registers accessible)");
        Ok(())
    }

    /// 清除 GUSB2PHYCFG.suspendusb20 位
    ///
    /// ⚠️ TRM 要求：应用程序必须在 power-on reset 后清除此位
    ///
    /// 根据 RK3588 TRM Chapter 13：
    /// > If it is set to 1, then the application must clear this bit after power-on reset.
    /// > Application needs to set it to 1 after the core initialization completes.
    ///
    /// suspendusb20 (bit[6]) 控制 USB2.0 PHY 的挂起状态：
    /// - 当设置为 1 时，USB2.0 PHY 进入挂起模式
    /// - 在 host mode，复位时此位被设置为 1，软件必须清除才能使 PHY 工作
    /// - PHY 处于挂起状态时，寄存器可能无法访问
    ///
    /// **正确的初始化序列**：
    /// 1. Power-on reset 后：suspendusb20 = 1 (PHY 挂起)
    /// 2. ⚠️ 调用此方法：suspendusb20 = 0 (使能 PHY)
    /// 3. 核心初始化期间：PHY 正常工作，寄存器可写
    /// 4. 初始化完成后：调用 set_suspend_usb20() (进入挂起模式)
    pub fn clear_suspend_usb20(&mut self) {
        log::info!("DWC3: Clearing GUSB2PHYCFG.suspendusb20 (bit[6]) - TRM requirement");

        let current = self.read_gusb2phy_cfg();
        log::debug!(
            "DWC3: GUSB2PHYCFG before clear_suspend_usb20: {:#010x}",
            current
        );

        // 检查 bit[6] 当前值
        let susphy = (current >> 6) & 0x1;
        if susphy == 1 {
            log::warn!("⚠ DWC3: SUSPHY (bit[6]) is SET - PHY is in suspend mode!");
            log::warn!("⚠ DWC3: This may prevent PHY register access");
        }

        // 清除 bit[6]
        let new_value = current & !(1 << 6);
        self.write_gusb2phy_cfg(new_value);

        // 验证写入
        let updated = self.read_gusb2phy_cfg();
        let new_susphy = (updated >> 6) & 0x1;

        log::debug!(
            "DWC3: GUSB2PHYCFG after clear_suspend_usb20: {:#010x}",
            updated
        );

        if new_susphy == 0 {
            log::info!("✓ DWC3: GUSB2PHYCFG.suspendusb20 cleared successfully");
            log::info!("✓ DWC3: USB2 PHY should now be active and registers accessible");
        } else {
            log::error!("❌ DWC3: Failed to clear GUSB2PHYCFG.suspendusb20!");
            log::error!("❌ DWC3: PHY register may remain inaccessible");
        }
    }

    /// 设置 GUSB2PHYCFG.suspendusb20 位
    ///
    /// 在核心初始化完成后调用，使 PHY 进入挂起模式以节省功耗
    ///
    /// 根据 RK3588 TRM Chapter 13：
    /// > Application needs to set it to 1 after the core initialization completes.
    pub fn set_suspend_usb20(&mut self) {
        log::info!("DWC3: Setting GUSB2PHYCFG.suspendusb20 (bit[6]) for power saving");

        let current = self.read_gusb2phy_cfg();
        log::debug!(
            "DWC3: GUSB2PHYCFG before set_suspend_usb20: {:#010x}",
            current
        );

        // 设置 bit[6]
        let new_value = current | (1 << 6);
        self.write_gusb2phy_cfg(new_value);

        // 验证写入
        let updated = self.read_gusb2phy_cfg();
        let new_susphy = (updated >> 6) & 0x1;

        log::debug!(
            "DWC3: GUSB2PHYCFG after set_suspend_usb20: {:#010x}",
            updated
        );

        if new_susphy == 1 {
            log::info!("✓ DWC3: GUSB2PHYCFG.suspendusb20 set successfully");
            log::info!("✓ DWC3: USB2 PHY now in suspend mode (power saving)");
        } else {
            log::warn!("⚠ DWC3: Failed to set GUSB2PHYCFG.suspendusb20");
        }
    }

    /// 简单的毫秒级延时（使用忙等待）
    pub fn delay_ms(&self, ms: u32) {
        crate::osal::kernel::delay(core::time::Duration::from_millis(ms as _));
    }

    // ==================== DCTL 寄存器操作 ====================

    /// 读取 DCTL 寄存器
    pub fn read_dctl(&self) -> u32 {
        self.globals().dctl.get()
    }

    /// 设置 DCTL 寄存器
    pub fn write_dctl(&mut self, value: u32) {
        self.globals_mut().dctl.set(value);
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    /// 启动/停止设备
    ///
    /// 参考 U-Boot drivers/usb/dwc3/core.c:dwc3_gadget_run()
    pub fn set_run_stop(&mut self, run: bool) {
        if run {
            self.globals_mut().dctl.modify(DCTL::RUN_STOP::Run);
            info!("DWC3: Device started (RUN_STOP=1)");
        } else {
            self.globals_mut().dctl.modify(DCTL::RUN_STOP::Stop);
            info!("DWC3: Device stopped (RUN_STOP=0)");
        }

        // 等待设备启动/停止完成（参考 U-Boot 的 100ms 延时）
        self.delay_ms(100);
    }

    /// 设置 U1 低功耗状态
    pub fn set_u1_enable(&mut self, init: bool, accept: bool) {
        self.globals_mut()
            .dctl
            .modify(DCTL::INITU1ENA.val(init as u32) + DCTL::ACCEPTU1ENA.val(accept as u32));

        log::debug!(
            "DWC3: U1 power state - INITU1ENA={}, ACCEPTU1ENA={}",
            init,
            accept
        );
    }

    /// 设置 U2 低功耗状态
    pub fn set_u2_enable(&mut self, init: bool, accept: bool) {
        self.globals_mut()
            .dctl
            .modify(DCTL::INITU2ENA.val(init as u32) + DCTL::ACCEPTU2ENA.val(accept as u32));

        log::debug!(
            "DWC3: U2 power state - INITU2ENA={}, ACCEPTU2ENA={}",
            init,
            accept
        );
    }

    /// 请求链路状态改变
    ///
    /// 参考 U-Boot drivers/usb/dwc3/gadget.c:dwc3_gadget_set_link_state()
    pub fn set_link_state(&mut self, state: u32) {
        // 清除 ULSTCHNGREQ 字段
        self.globals_mut().dctl.modify(DCTL::ULSTCHNGREQ.val(state));

        log::info!("DWC3: Link state change requested to state={}", state);

        // 等待状态改变完成
        self.delay_ms(5);
    }

    /// 清除测试控制模式
    ///
    /// 参考 U-Boot drivers/usb/dwc3/gadget.c:dwc3_gadget_set_test_mode()
    pub fn clear_test_mode(&mut self) {
        self.globals_mut().dctl.modify(DCTL::TSTCTRL_MASK.val(0));
        log::info!("DWC3: Test mode cleared");
    }

    /// 设置 HIRD 阈值
    ///
    /// HIRD (Host Initiated Remote Wakeup) 阈值用于远程唤醒延迟控制
    pub fn set_hird_threshold(&mut self, threshold: u32) {
        self.globals_mut()
            .dctl
            .modify(DCTL::HIRD_THRES.val(threshold & 0x1f));
        log::debug!("DWC3: HIRD threshold set to {}", threshold);
    }

    /// 保持连接控制（用于休眠模式）
    pub fn set_keep_connect(&mut self, enable: bool) {
        if enable {
            self.globals_mut().dctl.modify(DCTL::KEEP_CONNECT::Enable);
            log::debug!("DWC3: Keep connect enabled");
        } else {
            self.globals_mut().dctl.modify(DCTL::KEEP_CONNECT::Disable);
            log::debug!("DWC3: Keep connect disabled");
        }
    }

    /// 读取当前运行状态
    pub fn is_running(&self) -> bool {
        let reg = self.read_dctl();
        (reg >> 31) & 0x1 == 1
    }

    /// 读取当前 U1/U2 使能状态
    pub fn read_u1u2_status(&self) -> (bool, bool, bool, bool) {
        let reg = self.read_dctl();
        let init_u2 = (reg >> 12) & 0x1 == 1;
        let accept_u2 = (reg >> 11) & 0x1 == 1;
        let init_u1 = (reg >> 10) & 0x1 == 1;
        let accept_u1 = (reg >> 9) & 0x1 == 1;

        (init_u1, accept_u1, init_u2, accept_u2)
    }
}
