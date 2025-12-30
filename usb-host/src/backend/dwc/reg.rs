//! DWC3 寄存器定义
//! 基于 Linux drivers/usb/dwc3/core.h

use core::hint::spin_loop;
use core::sync::atomic::Ordering;

use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};
use tock_registers::{
    register_bitfields,
    registers::{ReadOnly, ReadWrite},
};

/// DWC3 全局寄存器基址偏移 (相对于 xHCI 寄存器区域)
const DWC3_GLOBALS_REGS_START: usize = 0xc100;

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
}

// =============================================================================
// 寄存器位字段定义
// =============================================================================

/// Global Control Register (GCTL) - 0xc110
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

/// SNPSID Register (GSNPSID) - 0xc120 (只读)
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
    fn globals(&self) -> &'static Dwc3Registers {
        let addr = self.base + DWC3_GLOBALS_REGS_START;
        unsafe { &*(addr as *const Dwc3Registers) }
    }

    /// 获取可变的全局寄存器
    fn globals_mut(&mut self) -> &'static mut Dwc3Registers {
        let addr = self.base + DWC3_GLOBALS_REGS_START;
        unsafe { &mut *(addr as *mut Dwc3Registers) }
    }

    // ==================== 寄存器操作封装 ====================

    /// 读取 SNPSID 寄存器（完整值）
    pub fn read_snpsid(&self) -> u32 {
        self.globals().gsnpsid.get()
    }

    /// 读取 SNPSID 的产品 ID
    pub fn read_product_id(&self) -> u16 {
        self.globals().gsnpsid.read(GSNPSID::PRODUCT_ID) as u16
    }

    /// 读取 SNPSID 的版本号
    pub fn read_revision(&self) -> u16 {
        self.globals().gsnpsid.read(GSNPSID::REVISION) as u16
    }

    /// 读取 GCTL 寄存器
    pub fn read_gctl(&self) -> u32 {
        self.globals().gctl.get()
    }

    /// 设置 GCTL 寄存器
    pub fn write_gctl(&mut self, value: u32) {
        self.globals_mut().gctl.set(value);
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
        // 使用 modify 方法直接修改寄存器位字段
        // 设置 U2SSINP3OK=1 (bit 15)
        self.globals_mut()
            .gusb3pipectl0
            .modify(GUSB3PIPECTL::U2SSINP3OK.val(1));
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

    // ==================== 高级方法封装 ====================

    /// 验证 SNPSID 寄存器
    ///
    /// 返回 (ip_id, product_id, revision)
    pub fn verify_snpsid(&self) -> (u16, u16, u16) {
        let snpsid_full = self.read_snpsid();
        let product_id = self.read_product_id();
        let revision = self.read_revision();
        let ip_id = (snpsid_full >> 16) & 0xffff;

        log::info!(
            "DWC3 SNPSID: full={:#010x}, ip_id={:#06x}, product_id={:#06x}, revision={:#06x}",
            snpsid_full,
            ip_id,
            product_id,
            revision
        );

        match ip_id {
            0x5533 => {
                log::info!("Detected DWC_usb3 controller");
            }
            0x3331 => {
                log::info!("Detected DWC_usb31 controller");
            }
            0x3332 => {
                log::info!("Detected DWC_usb32 controller");
            }
            _ => {
                log::warn!(
                    "Unknown DWC3 IP ID: {:#06x} (expected 0x5533, 0x3331, or 0x3332)",
                    ip_id
                );
                log::warn!("Continuing with initialization - may be a custom or FPGA variant");
            }
        }

        (ip_id as u16, product_id, revision)
    }

    /// 配置 GCTL 寄存器
    ///
    /// 参考 Linux drivers/usb/dwc3/core.c:dwc3_core_init()
    pub fn setup_gctl(&mut self) {
        log::info!("DWC3: Configuring GCTL");

        let current = self.read_gctl();
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

        self.write_gctl(reg);

        let updated = self.read_gctl();
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

        let current = self.read_gctl();
        log::debug!("DWC3: Current GCTL: {:#010x}", current);

        let current_prtcap = (current >> 12) & 0x3;
        log::debug!(
            "DWC3: Current PRTCAPDIR: {} (0=Device, 1=Host, 2=Device, 3=OTG)",
            current_prtcap
        );

        // 设置为 HOST 模式
        self.set_prtcap_dir(1); // 1 = Host mode

        let updated = self.read_gctl();
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
    /// 参考 Linux 内核 drivers/usb/dwc3/core.c:dwc3_phy_setup()
    /// 和 u-boot drivers/usb/host/xhci-dwc3.c
    pub fn setup_phy(&mut self) {
        log::info!("DWC3: Starting PHY configuration");

        // === 步骤 0: 读取并记录初始状态 ===
        let gctl_init = self.read_gctl();
        let gusb2_init = self.read_gusb2phy_cfg();
        let gusb3_init = self.read_gusb3pipe_ctl();

        log::info!("DWC3: Initial register states:");
        log::info!("  GCTL:          {:#010x}", gctl_init);
        log::info!("  GUSB2PHYCFG:   {:#010x}", gusb2_init);
        log::info!("  GUSB3PIPECTL:   {:#010x}", gusb3_init);

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
        log::debug!("DWC3: GUSB3PIPECTL after:  {:#010x}", updated);

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

        log::info!("DWC3: Final PHY register states:");
        log::info!("  GUSB2PHYCFG:   {:#010x}", gusb2_final);
        log::info!("  GUSB3PIPECTL:   {:#010x}", gusb3_final);

        // 验证 PHY 复位已解除
        let (usb2_reset, usb3_reset) = self.is_phy_in_reset();
        if usb2_reset {
            log::error!("DWC3: USB2 PHY still in reset!");
        }
        if usb3_reset {
            log::error!("DWC3: USB3 PHY still in reset!");
        }

        log::info!("DWC3: PHY configuration complete");
    }

    /// 简单的毫秒级延时（使用忙等待）
    fn delay_ms(&self, ms: u32) {
        const LOOPS_PER_MS: u32 = 50000;
        let total_loops = ms * LOOPS_PER_MS;
        for _ in 0..total_loops {
            spin_loop();
        }
        core::sync::atomic::fence(Ordering::SeqCst);
    }
}
