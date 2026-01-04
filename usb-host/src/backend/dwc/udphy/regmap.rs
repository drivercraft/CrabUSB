#[derive(Clone, Copy)]
pub struct Regmap(usize);

impl Regmap {
    pub fn new(base: Mmio) -> Self {
        Self(base.as_ptr() as usize)
    }

    pub fn grfreg_write(&self, reg: &UdphyGrfReg, en: bool) {
        let mut tmp = if en { reg.enable } else { reg.disable };
        let mask = genmask(reg.bitend, reg.bitstart) as u32;
        let val = (tmp << reg.bitstart) | (mask << 16);
        self.reg_write(reg.offset, val);
    }

    fn reg_read(&self, offset: u32) -> u32 {
        let addr = (self.0 + offset as usize) as *const u32;
        unsafe { addr.read_volatile() }
    }

    fn reg_write(&self, offset: u32, val: u32) {
        let addr = (self.0 + offset as usize) as *mut u32;
        unsafe {
            addr.write_volatile(val);
        }
    }

    pub fn multi_reg_write(&self, regs: &[RegSequence]) {
        for reg in regs {
            self.reg_write(reg.reg, reg.def);
        }
    }

    pub fn update_bits(&self, offset: u32, mask: u32, val: u32) {
        let current = self.reg_read(offset);
        let new = (current & !mask) | val;
        self.reg_write(offset, new);
    }
}

/// 寄存器配置项
#[derive(Debug, Clone, Copy)]
pub struct RegSequence {
    reg: u32,
    def: u32,
}

// =============================================================================
// 寄存器初始化序列 (基于 u-boot drivers/phy/phy-rockchip-usbdp.c)
// =============================================================================

/// RK3588 USBDP PHY 24MHz 参考时钟配置序列
///
/// 参考 u-boot: rk3588_udphy_24m_refclk_cfg
/// 位置: drivers/phy/phy-rockchip-usbdp.c:226-263
pub const RK3588_UDPHY_24M_REFCLK_CFG: &[RegSequence] = &[
    // PMA 寄存器配置块 0
    RegSequence {
        reg: 0x0090,
        def: 0x68,
    },
    RegSequence {
        reg: 0x0094,
        def: 0x68,
    },
    RegSequence {
        reg: 0x0128,
        def: 0x24,
    },
    RegSequence {
        reg: 0x012c,
        def: 0x44,
    },
    RegSequence {
        reg: 0x0130,
        def: 0x3f,
    },
    RegSequence {
        reg: 0x0134,
        def: 0x44,
    },
    RegSequence {
        reg: 0x015c,
        def: 0xa9,
    },
    RegSequence {
        reg: 0x0160,
        def: 0x71,
    },
    RegSequence {
        reg: 0x0164,
        def: 0x71,
    },
    RegSequence {
        reg: 0x0168,
        def: 0xa9,
    },
    RegSequence {
        reg: 0x0174,
        def: 0xa9,
    },
    RegSequence {
        reg: 0x0178,
        def: 0x71,
    },
    RegSequence {
        reg: 0x017c,
        def: 0x71,
    },
    RegSequence {
        reg: 0x0180,
        def: 0xa9,
    },
    RegSequence {
        reg: 0x018c,
        def: 0x41,
    },
    RegSequence {
        reg: 0x0190,
        def: 0x00,
    },
    RegSequence {
        reg: 0x0194,
        def: 0x05,
    },
    RegSequence {
        reg: 0x01ac,
        def: 0x2a,
    },
    RegSequence {
        reg: 0x01b0,
        def: 0x17,
    },
    RegSequence {
        reg: 0x01b4,
        def: 0x17,
    },
    RegSequence {
        reg: 0x01b8,
        def: 0x2a,
    },
    RegSequence {
        reg: 0x01c8,
        def: 0x04,
    },
    RegSequence {
        reg: 0x01cc,
        def: 0x08,
    },
    RegSequence {
        reg: 0x01d0,
        def: 0x08,
    },
    RegSequence {
        reg: 0x01d4,
        def: 0x04,
    },
    RegSequence {
        reg: 0x01d8,
        def: 0x20,
    },
    RegSequence {
        reg: 0x01dc,
        def: 0x01,
    },
    RegSequence {
        reg: 0x01e0,
        def: 0x09,
    },
    RegSequence {
        reg: 0x01e4,
        def: 0x03,
    },
    RegSequence {
        reg: 0x01f0,
        def: 0x29,
    },
    RegSequence {
        reg: 0x01f4,
        def: 0x02,
    },
    RegSequence {
        reg: 0x01f8,
        def: 0x02,
    },
    RegSequence {
        reg: 0x01fc,
        def: 0x29,
    },
    RegSequence {
        reg: 0x0208,
        def: 0x2a,
    },
    RegSequence {
        reg: 0x020c,
        def: 0x17,
    },
    RegSequence {
        reg: 0x0210,
        def: 0x17,
    },
    RegSequence {
        reg: 0x0214,
        def: 0x2a,
    },
    RegSequence {
        reg: 0x0224,
        def: 0x20,
    },
    RegSequence {
        reg: 0x03f0,
        def: 0x0a,
    },
    RegSequence {
        reg: 0x03f4,
        def: 0x07,
    },
    RegSequence {
        reg: 0x03f8,
        def: 0x07,
    },
    RegSequence {
        reg: 0x03fc,
        def: 0x0c,
    },
    RegSequence {
        reg: 0x0404,
        def: 0x12,
    },
    RegSequence {
        reg: 0x0408,
        def: 0x1a,
    },
    RegSequence {
        reg: 0x040c,
        def: 0x1a,
    },
    RegSequence {
        reg: 0x0410,
        def: 0x3f,
    },
    // Lane 0 和 Lane 1 配置
    RegSequence {
        reg: 0x0ce0,
        def: 0x68,
    },
    RegSequence {
        reg: 0x0ce8,
        def: 0xd0,
    },
    RegSequence {
        reg: 0x0cf0,
        def: 0x87,
    },
    RegSequence {
        reg: 0x0cf8,
        def: 0x70,
    },
    RegSequence {
        reg: 0x0d00,
        def: 0x70,
    },
    RegSequence {
        reg: 0x0d08,
        def: 0xa9,
    },
    // Lane 2 和 Lane 3 配置
    RegSequence {
        reg: 0x1ce0,
        def: 0x68,
    },
    RegSequence {
        reg: 0x1ce8,
        def: 0xd0,
    },
    RegSequence {
        reg: 0x1cf0,
        def: 0x87,
    },
    RegSequence {
        reg: 0x1cf8,
        def: 0x70,
    },
    RegSequence {
        reg: 0x1d00,
        def: 0x70,
    },
    RegSequence {
        reg: 0x1d08,
        def: 0xa9,
    },
    // Lane 0 Tx 驱动配置
    RegSequence {
        reg: 0x0a3c,
        def: 0xd0,
    },
    RegSequence {
        reg: 0x0a44,
        def: 0xd0,
    },
    RegSequence {
        reg: 0x0a48,
        def: 0x01,
    },
    RegSequence {
        reg: 0x0a4c,
        def: 0x0d,
    },
    RegSequence {
        reg: 0x0a54,
        def: 0xe0,
    },
    RegSequence {
        reg: 0x0a5c,
        def: 0xe0,
    },
    RegSequence {
        reg: 0x0a64,
        def: 0xa8,
    },
    // Lane 2 Tx 驱动配置
    RegSequence {
        reg: 0x1a3c,
        def: 0xd0,
    },
    RegSequence {
        reg: 0x1a44,
        def: 0xd0,
    },
    RegSequence {
        reg: 0x1a48,
        def: 0x01,
    },
    RegSequence {
        reg: 0x1a4c,
        def: 0x0d,
    },
    RegSequence {
        reg: 0x1a54,
        def: 0xe0,
    },
    RegSequence {
        reg: 0x1a5c,
        def: 0xe0,
    },
    RegSequence {
        reg: 0x1a64,
        def: 0xa8,
    },
];

/// RK3588 USBDP PHY 初始化序列
///
/// 参考 u-boot: rk3588_udphy_init_sequence
/// 位置: drivers/phy/phy-rockchip-usbdp.c:265-299
pub const RK3588_UDPHY_INIT_SEQUENCE: &[RegSequence] = &[
    // CMN 和 Lane 0 初始化
    RegSequence {
        reg: 0x0104,
        def: 0x44,
    },
    RegSequence {
        reg: 0x0234,
        def: 0xE8,
    },
    RegSequence {
        reg: 0x0248,
        def: 0x44,
    },
    RegSequence {
        reg: 0x028C,
        def: 0x18,
    },
    RegSequence {
        reg: 0x081C,
        def: 0xE5,
    },
    RegSequence {
        reg: 0x0878,
        def: 0x00,
    },
    RegSequence {
        reg: 0x0994,
        def: 0x1C,
    },
    RegSequence {
        reg: 0x0AF0,
        def: 0x00,
    },
    // Lane 2 初始化
    RegSequence {
        reg: 0x181C,
        def: 0xE5,
    },
    RegSequence {
        reg: 0x1878,
        def: 0x00,
    },
    RegSequence {
        reg: 0x1994,
        def: 0x1C,
    },
    RegSequence {
        reg: 0x1AF0,
        def: 0x00,
    },
    // CMN 配置
    RegSequence {
        reg: 0x0428,
        def: 0x60,
    },
    RegSequence {
        reg: 0x0D58,
        def: 0x33,
    },
    RegSequence {
        reg: 0x1D58,
        def: 0x33,
    },
    // Lane 0 配置
    RegSequence {
        reg: 0x0990,
        def: 0x74,
    },
    RegSequence {
        reg: 0x0D64,
        def: 0x17,
    },
    RegSequence {
        reg: 0x08C8,
        def: 0x13,
    },
    // Lane 2 配置
    RegSequence {
        reg: 0x1990,
        def: 0x74,
    },
    RegSequence {
        reg: 0x1D64,
        def: 0x17,
    },
    RegSequence {
        reg: 0x18C8,
        def: 0x13,
    },
    // Lane 0 RX/TX 配置
    RegSequence {
        reg: 0x0D90,
        def: 0x40,
    },
    RegSequence {
        reg: 0x0DA8,
        def: 0x40,
    },
    RegSequence {
        reg: 0x0DC0,
        def: 0x40,
    },
    RegSequence {
        reg: 0x0DD8,
        def: 0x40,
    },
    // Lane 2 RX/TX 配置
    RegSequence {
        reg: 0x1D90,
        def: 0x40,
    },
    RegSequence {
        reg: 0x1DA8,
        def: 0x40,
    },
    RegSequence {
        reg: 0x1DC0,
        def: 0x40,
    },
    RegSequence {
        reg: 0x1DD8,
        def: 0x40,
    },
    // CMN PLL 配置
    RegSequence {
        reg: 0x03C0,
        def: 0x30,
    },
    RegSequence {
        reg: 0x03C4,
        def: 0x06,
    },
    RegSequence {
        reg: 0x0E10,
        def: 0x00,
    },
    RegSequence {
        reg: 0x1E10,
        def: 0x00,
    },
    RegSequence {
        reg: 0x043C,
        def: 0x0F,
    },
    RegSequence {
        reg: 0x0D2C,
        def: 0xFF,
    },
    RegSequence {
        reg: 0x1D2C,
        def: 0xFF,
    },
    RegSequence {
        reg: 0x0D34,
        def: 0x0F,
    },
    RegSequence {
        reg: 0x1D34,
        def: 0x0F,
    },
    // Lane 0 精细配置
    RegSequence {
        reg: 0x08FC,
        def: 0x2A,
    },
    RegSequence {
        reg: 0x0914,
        def: 0x28,
    },
    RegSequence {
        reg: 0x0A30,
        def: 0x03,
    },
    RegSequence {
        reg: 0x0E38,
        def: 0x05,
    },
    RegSequence {
        reg: 0x0ECC,
        def: 0x27,
    },
    RegSequence {
        reg: 0x0ED0,
        def: 0x22,
    },
    RegSequence {
        reg: 0x0ED4,
        def: 0x26,
    },
    // Lane 2 精细配置
    RegSequence {
        reg: 0x18FC,
        def: 0x2A,
    },
    RegSequence {
        reg: 0x1914,
        def: 0x28,
    },
    RegSequence {
        reg: 0x1A30,
        def: 0x03,
    },
    RegSequence {
        reg: 0x1E38,
        def: 0x05,
    },
    RegSequence {
        reg: 0x1ECC,
        def: 0x27,
    },
    RegSequence {
        reg: 0x1ED0,
        def: 0x22,
    },
    RegSequence {
        reg: 0x1ED4,
        def: 0x26,
    },
    // CMN 最终配置
    RegSequence {
        reg: 0x0048,
        def: 0x0F,
    },
    RegSequence {
        reg: 0x0060,
        def: 0x3C,
    },
    RegSequence {
        reg: 0x0064,
        def: 0xF7,
    },
    RegSequence {
        reg: 0x006C,
        def: 0x20,
    },
    RegSequence {
        reg: 0x0070,
        def: 0x7D,
    },
    RegSequence {
        reg: 0x0074,
        def: 0x68,
    },
    RegSequence {
        reg: 0x0AF4,
        def: 0x1A,
    },
    RegSequence {
        reg: 0x1AF4,
        def: 0x1A,
    },
    RegSequence {
        reg: 0x0440,
        def: 0x3F,
    },
    RegSequence {
        reg: 0x10D4,
        def: 0x08,
    },
    RegSequence {
        reg: 0x20D4,
        def: 0x08,
    },
    RegSequence {
        reg: 0x00D4,
        def: 0x30,
    },
    RegSequence {
        reg: 0x0024,
        def: 0x6e,
    },
];
