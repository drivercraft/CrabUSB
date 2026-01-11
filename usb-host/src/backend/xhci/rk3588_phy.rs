//! RK3588 USBDP Combo PHY and GPIO VBUS control
//!
//! This module provides PHY initialization for USB3 ports on RK3588 SoC,
//! plus GPIO-based VBUS power control for the GL3523 hub workaround.

#![allow(dead_code)] // Complete API for RK3588 USB subsystem

use core::ptr::{read_volatile, write_volatile};
use log::{debug, warn};

use super::delay::{delay_ms, delay_us};

pub const USBDPPHY1_BASE: usize = 0xFED90000;
pub const USBDPPHY1_PMA_BASE: usize = USBDPPHY1_BASE + 0x8000;
pub const USBDPPHY1_GRF_BASE: usize = 0xFD5CC000;
pub const USB2PHY1_GRF_BASE: usize = 0xFD5D4000;
pub const USB_GRF_BASE: usize = 0xFD5AC000;

const USBDPPHY_GRF_CON0: usize = 0x0000;
const USBDPPHY_GRF_CON1: usize = 0x0004;
const USB_GRF_USB3OTG1_CON0: usize = 0x0030;
const USB_GRF_USB3OTG1_CON1: usize = 0x0034;

const USBDPPHY_GRF_LOW_PWRN_BIT: u32 = 13;
const USBDPPHY_GRF_RX_LFPS_BIT: u32 = 14;

const USB3OTG1_CFG_ENABLE: u32 = 0x1100;
const USB3OTG1_CFG_DISABLE: u32 = 0x0188;
const USB3OTG1_CFG_MASK: u32 = 0xFFFF;

const CMN_LANE_MUX_AND_EN_OFFSET: usize = 0x0288;
const CMN_DP_LANE_MUX_N: fn(u32) -> u32 = |n| 1 << (n + 4);
const CMN_DP_LANE_EN_N: fn(u32) -> u32 = |n| 1 << n;
const CMN_DP_LANE_MUX_ALL: u32 = 0xF0;
const CMN_DP_LANE_EN_ALL: u32 = 0x0F;

const PHY_LANE_MUX_USB: u32 = 0;
const PHY_LANE_MUX_DP: u32 = 1;

const CMN_DP_RSTN_OFFSET: usize = 0x038C;
const CMN_DP_INIT_RSTN: u32 = 1 << 3;

const CMN_ANA_LCPLL_DONE_OFFSET: usize = 0x0350;
const CMN_ANA_LCPLL_AFC_DONE: u32 = 1 << 6;
const CMN_ANA_LCPLL_LOCK_DONE: u32 = 1 << 7;

const TRSV_LN0_MON_RX_CDR_DONE_OFFSET: usize = 0x0B84;
const TRSV_LN0_MON_RX_CDR_LOCK_DONE: u32 = 1 << 0;

const TRSV_LN2_MON_RX_CDR_DONE_OFFSET: usize = 0x1B84;
const TRSV_LN2_MON_RX_CDR_LOCK_DONE: u32 = 1 << 0;

static RK3588_UDPHY_24M_REFCLK_CFG: &[(u16, u8)] = &[
    (0x0090, 0x68), (0x0094, 0x68),
    (0x0128, 0x24), (0x012c, 0x44),
    (0x0130, 0x3f), (0x0134, 0x44),
    (0x015c, 0xa9), (0x0160, 0x71),
    (0x0164, 0x71), (0x0168, 0xa9),
    (0x0174, 0xa9), (0x0178, 0x71),
    (0x017c, 0x71), (0x0180, 0xa9),
    (0x018c, 0x41), (0x0190, 0x00),
    (0x0194, 0x05), (0x01ac, 0x2a),
    (0x01b0, 0x17), (0x01b4, 0x17),
    (0x01b8, 0x2a), (0x01c8, 0x04),
    (0x01cc, 0x08), (0x01d0, 0x08),
    (0x01d4, 0x04), (0x01d8, 0x20),
    (0x01dc, 0x01), (0x01e0, 0x09),
    (0x01e4, 0x03), (0x01f0, 0x29),
    (0x01f4, 0x02), (0x01f8, 0x02),
    (0x01fc, 0x29), (0x0208, 0x2a),
    (0x020c, 0x17), (0x0210, 0x17),
    (0x0214, 0x2a), (0x0224, 0x20),
    (0x03f0, 0x0d), (0x03f4, 0x09),
    (0x03f8, 0x09), (0x03fc, 0x0d),
    (0x0404, 0x0e), (0x0408, 0x14),
    (0x040c, 0x14), (0x0410, 0x3b),
    (0x0ce0, 0x68), (0x0ce8, 0xd0),
    (0x0cf0, 0x87), (0x0cf8, 0x70),
    (0x0d00, 0x70), (0x0d08, 0xa9),
    (0x1ce0, 0x68), (0x1ce8, 0xd0),
    (0x1cf0, 0x87), (0x1cf8, 0x70),
    (0x1d00, 0x70), (0x1d08, 0xa9),
    (0x0a3c, 0xd0), (0x0a44, 0xd0),
    (0x0a48, 0x01), (0x0a4c, 0x0d),
    (0x0a54, 0xe0), (0x0a5c, 0xe0),
    (0x0a64, 0xa8), (0x1a3c, 0xd0),
    (0x1a44, 0xd0), (0x1a48, 0x01),
    (0x1a4c, 0x0d), (0x1a54, 0xe0),
    (0x1a5c, 0xe0), (0x1a64, 0xa8),
];

static RK3588_UDPHY_INIT_SEQUENCE: &[(u16, u8)] = &[
    (0x0104, 0x44), (0x0234, 0xE8),
    (0x0248, 0x44), (0x028C, 0x18),
    (0x081C, 0xE5), (0x0878, 0x00),
    (0x0994, 0x1C), (0x0AF0, 0x00),
    (0x181C, 0xE5), (0x1878, 0x00),
    (0x1994, 0x1C), (0x1AF0, 0x00),
    (0x0428, 0x60), (0x0D58, 0x33),
    (0x1D58, 0x33), (0x0990, 0x74),
    (0x0D64, 0x17), (0x08C8, 0x13),
    (0x1990, 0x74), (0x1D64, 0x17),
    (0x18C8, 0x13), (0x0D90, 0x40),
    (0x0DA8, 0x40), (0x0DC0, 0x40),
    (0x0DD8, 0x40), (0x1D90, 0x40),
    (0x1DA8, 0x40), (0x1DC0, 0x40),
    (0x1DD8, 0x40), (0x03C0, 0x30),
    (0x03C4, 0x06), (0x0E10, 0x00),
    (0x1E10, 0x00), (0x043C, 0x0F),
    (0x0D2C, 0xFF), (0x1D2C, 0xFF),
    (0x0D34, 0x0F), (0x1D34, 0x0F),
    (0x08FC, 0x2A), (0x0914, 0x28),
    (0x0A30, 0x03), (0x0E38, 0x05),
    (0x0ECC, 0x27), (0x0ED0, 0x22),
    (0x0ED4, 0x26), (0x18FC, 0x2A),
    (0x1914, 0x28), (0x1A30, 0x03),
    (0x1E38, 0x05), (0x1ECC, 0x27),
    (0x1ED0, 0x22), (0x1ED4, 0x26),
    (0x0048, 0x0F), (0x0060, 0x3C),
    (0x0064, 0xF7), (0x006C, 0x20),
    (0x0070, 0x7D), (0x0074, 0x68),
    (0x0AF4, 0x1A), (0x1AF4, 0x1A),
    (0x0440, 0x3F), (0x10D4, 0x08),
    (0x20D4, 0x08), (0x00D4, 0x30),
    (0x0024, 0x6e),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UdphyMode {
    Usb = 1,
    Dp = 2,
    UsbDp = 3,
}

#[derive(Debug, Clone)]
pub struct LaneMuxConfig {
    pub lane_mux_sel: [u32; 4],
}

impl Default for LaneMuxConfig {
    fn default() -> Self {
        Self {
            lane_mux_sel: [PHY_LANE_MUX_USB, PHY_LANE_MUX_USB, PHY_LANE_MUX_DP, PHY_LANE_MUX_DP],
        }
    }
}

pub struct Rk3588UsbdpPhy {
    pma_base: usize,
    udphygrf_base: usize,
    usbgrf_base: usize,
    mode: UdphyMode,
    lane_mux: LaneMuxConfig,
    flip: bool,
}

impl Rk3588UsbdpPhy {
    pub unsafe fn new_port1() -> Self {
        Self {
            pma_base: USBDPPHY1_PMA_BASE,
            udphygrf_base: USBDPPHY1_GRF_BASE,
            usbgrf_base: USB_GRF_BASE,
            mode: UdphyMode::UsbDp,
            lane_mux: LaneMuxConfig::default(),
            flip: false,
        }
    }

    pub fn init(&self) -> Result<(), &'static str> {
        debug!("RK3588 USBDP PHY: Starting initialization for Port 1");

        // Step 1: Enable rx_lfps for USB
        if self.mode == UdphyMode::Usb || self.mode == UdphyMode::UsbDp {
            self.grf_write(self.udphygrf_base, USBDPPHY_GRF_CON1, USBDPPHY_GRF_RX_LFPS_BIT, true);
        }

        // Step 2: Power on PMA (set low_pwrn high)
        self.grf_write(self.udphygrf_base, USBDPPHY_GRF_CON1, USBDPPHY_GRF_LOW_PWRN_BIT, true);

        delay_us(100);

        // Step 3: Write init sequence to PMA
        for &(offset, value) in RK3588_UDPHY_INIT_SEQUENCE {
            self.pma_write(offset as usize, value as u32);
        }

        // Step 4: Write 24MHz reference clock configuration
        for &(offset, value) in RK3588_UDPHY_24M_REFCLK_CFG {
            self.pma_write(offset as usize, value as u32);
        }

        // Step 5: Configure lane mux
        let mut lane_mux_val = 0u32;
        for (i, &mux) in self.lane_mux.lane_mux_sel.iter().enumerate() {
            if mux == PHY_LANE_MUX_DP {
                lane_mux_val |= CMN_DP_LANE_MUX_N(i as u32);
            }
        }
        self.pma_update(CMN_LANE_MUX_AND_EN_OFFSET, 
                       CMN_DP_LANE_MUX_ALL | CMN_DP_LANE_EN_ALL,
                       lane_mux_val);

        // Step 6: Deassert init reset for USB mode
        if self.mode == UdphyMode::Usb || self.mode == UdphyMode::UsbDp {
            let current = self.pma_read(CMN_DP_RSTN_OFFSET);
            self.pma_write(CMN_DP_RSTN_OFFSET, current | CMN_DP_INIT_RSTN);
        }

        delay_us(1);

        // Step 7: Wait for PLL lock
        if self.mode == UdphyMode::Usb || self.mode == UdphyMode::UsbDp {
            self.wait_for_pll_lock()?;
        }

        // Step 8: Enable USB3 port in USB GRF
        self.usb3_port_enable(true);

        debug!("RK3588 USBDP PHY: Initialization complete");
        Ok(())
    }

    fn wait_for_pll_lock(&self) -> Result<(), &'static str> {
        let mut timeout = 200;
        loop {
            let val = self.pma_read(CMN_ANA_LCPLL_DONE_OFFSET);
            if (val & CMN_ANA_LCPLL_AFC_DONE) != 0 && (val & CMN_ANA_LCPLL_LOCK_DONE) != 0 {
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                warn!("RK3588 USBDP PHY: LCPLL lock timeout (val=0x{:02x})", val);
                return Err("LCPLL lock timeout");
            }
            delay_us(1000);
        }

        let cdr_offset = if self.flip {
            TRSV_LN2_MON_RX_CDR_DONE_OFFSET
        } else {
            TRSV_LN0_MON_RX_CDR_DONE_OFFSET
        };
        let cdr_done_bit = if self.flip {
            TRSV_LN2_MON_RX_CDR_LOCK_DONE
        } else {
            TRSV_LN0_MON_RX_CDR_LOCK_DONE
        };

        timeout = 200;
        loop {
            let val = self.pma_read(cdr_offset);
            if (val & cdr_done_bit) != 0 {
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                warn!("RK3588 USBDP PHY: CDR lock timeout (val=0x{:02x})", val);
                break;
            }
            delay_us(1000);
        }

        Ok(())
    }

    fn usb3_port_enable(&self, enable: bool) {
        let val = if enable { USB3OTG1_CFG_ENABLE } else { USB3OTG1_CFG_DISABLE };
        let write_val = (USB3OTG1_CFG_MASK << 16) | val;
        unsafe {
            let ptr = (self.usbgrf_base + USB_GRF_USB3OTG1_CON1) as *mut u32;
            write_volatile(ptr, write_val);
        }
    }

    fn grf_write(&self, base: usize, offset: usize, bit: u32, set: bool) {
        let mask = 1u32 << bit;
        let val = if set { mask } else { 0 };
        let write_val = (mask << 16) | val;
        unsafe {
            let ptr = (base + offset) as *mut u32;
            write_volatile(ptr, write_val);
        }
    }

    fn pma_read(&self, offset: usize) -> u32 {
        unsafe {
            let ptr = (self.pma_base + offset) as *const u32;
            read_volatile(ptr)
        }
    }

    fn pma_write(&self, offset: usize, value: u32) {
        unsafe {
            let ptr = (self.pma_base + offset) as *mut u32;
            write_volatile(ptr, value);
        }
    }

    fn pma_update(&self, offset: usize, mask: u32, value: u32) {
        let current = self.pma_read(offset);
        let new_val = (current & !mask) | (value & mask);
        self.pma_write(offset, new_val);
    }
}

pub unsafe fn init_rk3588_usbdp_phy_port1() -> Result<(), &'static str> {
    let phy = unsafe { Rk3588UsbdpPhy::new_port1() };
    phy.init()
}

pub fn is_rk3588_usb3_port1(xhci_base: usize) -> bool {
    xhci_base == 0xFC400000
}

const GPIO3_BASE: usize = 0xFEC40000;
const GPIO_SWPORT_DR_L: usize = 0x0000;
const GPIO_SWPORT_DDR_L: usize = 0x0008;
const GPIO3_B7_BIT: u32 = 1 << 15;
const WRITE_MASK_BIT15: u32 = 1 << 31;

pub struct Rk3588VbusGpio {
    gpio_base: usize,
    pin_bit: u32,
    write_mask: u32,
}

impl Rk3588VbusGpio {
    pub unsafe fn new_port1() -> Self {
        Self {
            gpio_base: GPIO3_BASE,
            pin_bit: GPIO3_B7_BIT,
            write_mask: WRITE_MASK_BIT15,
        }
    }

    fn configure_as_output(&self) {
        unsafe {
            let ddr_ptr = (self.gpio_base + GPIO_SWPORT_DDR_L) as *mut u32;
            write_volatile(ddr_ptr, self.write_mask | self.pin_bit);
        }
    }

    pub fn set_vbus(&self, enabled: bool) {
        self.configure_as_output();
        
        unsafe {
            let dr_ptr = (self.gpio_base + GPIO_SWPORT_DR_L) as *mut u32;
            let value = if enabled { self.pin_bit } else { 0 };
            write_volatile(dr_ptr, self.write_mask | value);
        }
    }

    pub fn get_vbus(&self) -> bool {
        unsafe {
            let dr_ptr = (self.gpio_base + GPIO_SWPORT_DR_L) as *const u32;
            (read_volatile(dr_ptr) & self.pin_bit) != 0
        }
    }

    pub fn toggle_vbus(&self, off_ms: u32, on_wait_ms: u32) {
        self.set_vbus(false);
        delay_ms(off_ms);
        
        self.set_vbus(true);
        delay_ms(on_wait_ms);
    }
}

pub unsafe fn toggle_vbus_port1(off_ms: u32, on_wait_ms: u32) {
    let gpio = unsafe { Rk3588VbusGpio::new_port1() };
    gpio.toggle_vbus(off_ms, on_wait_ms);
}

pub const VBUS_OFF_MS: u32 = 1000;
pub const VBUS_ON_WAIT_MS: u32 = 500;
