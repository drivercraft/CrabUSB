//! DWC3 寄存器定义
//! 基于 Linux drivers/usb/dwc3/core.h

use tock_registers::{register_bitfields, registers::ReadWrite};

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
    pub gsnpsid: ReadWrite<u32, GSNPSID::Register>,

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
    pub GCTL [
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
    pub GSTS [
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
    pub GSNPSID [
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
    pub GUSB2PHYCFG [
        /// PHY 软复位
        PHYSOFTRST OFFSET(31) NUMBITS(1) [
            Normal = 0,
            Reset = 1
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
    pub GUSB3PIPECTL [
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
        ]
    ]
];

/// Global Event Enable Register (GEVTEN) - 0xc114
register_bitfields![u32,
    pub GEVTEN [
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
    pub GUCTL1 [
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
    pub GUCTL [
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
    pub GGPIO [
        /// GPIO 方向
        GPIO_DIR OFFSET(16) NUMBITS(16) [],

        /// GPIO 数据
        GPIO_DATA OFFSET(0) NUMBITS(16) []
    ]
];

/// GUID Register (GUID) - 0xc128
register_bitfields![u32,
    pub GUID [
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
    pub fn globals(&self) -> &'static Dwc3Registers {
        let addr = self.base + DWC3_GLOBALS_REGS_START;
        unsafe { &*(addr as *const Dwc3Registers) }
    }

    /// 获取可变的全局寄存器
    pub fn globals_mut(&mut self) -> &'static mut Dwc3Registers {
        let addr = self.base + DWC3_GLOBALS_REGS_START;
        unsafe { &mut *(addr as *mut Dwc3Registers) }
    }
}
