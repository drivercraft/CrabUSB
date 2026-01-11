//! DWC3 (DesignWare USB3 Controller) initialization module
//!
//! This module provides DWC3 core initialization required for XHCI controllers
//! based on Synopsys DesignWare USB3 DRD IP, such as those found in RK3588.
//!
//! The DWC3 registers are located at offset 0xC100 from the XHCI base address.
//!
//! Reference: U-Boot drivers/usb/host/xhci-dwc3.c

#![allow(dead_code)] // Module contains complete API for future use

use core::ptr::NonNull;
use log::{debug, warn};

use super::delay::delay_ms;

/// DWC3 register offset from XHCI base address
pub const DWC3_REG_OFFSET: usize = 0xC100;

/// DWC3 Global registers structure
/// Located at XHCI_BASE + 0xC100
#[repr(C)]
pub struct Dwc3Regs {
    pub g_sbuscfg0: u32,     // 0x00
    pub g_sbuscfg1: u32,     // 0x04
    pub g_txthrcfg: u32,     // 0x08
    pub g_rxthrcfg: u32,     // 0x0C
    pub g_ctl: u32,          // 0x10 - Global Control Register
    _reserved1: u32,         // 0x14
    pub g_sts: u32,          // 0x18 - Global Status Register
    _reserved2: u32,         // 0x1C
    pub g_snpsid: u32,       // 0x20 - Synopsys ID Register
    pub g_gpio: u32,         // 0x24
    pub g_uid: u32,          // 0x28
    pub g_uctl: u32,         // 0x2C
    pub g_buserraddr_lo: u32, // 0x30
    pub g_buserraddr_hi: u32, // 0x34
    pub g_prtbimap_lo: u32,  // 0x38
    pub g_prtbimap_hi: u32,  // 0x3C
    pub g_hwparams0: u32,    // 0x40
    pub g_hwparams1: u32,    // 0x44
    pub g_hwparams2: u32,    // 0x48
    pub g_hwparams3: u32,    // 0x4C
    pub g_hwparams4: u32,    // 0x50
    pub g_hwparams5: u32,    // 0x54
    pub g_hwparams6: u32,    // 0x58
    pub g_hwparams7: u32,    // 0x5C
    pub g_dbgfifospace: u32, // 0x60
    pub g_dbgltssm: u32,     // 0x64
    pub g_dbglnmcc: u32,     // 0x68
    pub g_dbgbmu: u32,       // 0x6C
    pub g_dbglspmux: u32,    // 0x70
    pub g_dbglsp: u32,       // 0x74
    pub g_dbgepinfo0: u32,   // 0x78
    pub g_dbgepinfo1: u32,   // 0x7C
    pub g_prtbimap_hs_lo: u32, // 0x80
    pub g_prtbimap_hs_hi: u32, // 0x84
    pub g_prtbimap_fs_lo: u32, // 0x88
    pub g_prtbimap_fs_hi: u32, // 0x8C
    _reserved3: [u32; 28],   // 0x90-0xFF
    pub g_usb2phycfg: [u32; 16], // 0x100 - USB2 PHY Configuration
    pub g_usb2i2cctl: [u32; 16], // 0x140
    pub g_usb2phyacc: [u32; 16], // 0x180
    pub g_usb3pipectl: [u32; 16], // 0x1C0 - USB3 PIPE Control
    pub g_txfifosiz: [u32; 32], // 0x200
    pub g_rxfifosiz: [u32; 32], // 0x280
    // ... more registers follow
}

// DWC3 Synopsys ID masks
pub const DWC3_GSNPSID_MASK: u32 = 0xFFFF0000;
pub const DWC3_GSNPSID_CORE_3: u32 = 0x55330000;
pub const DWC3_GSNPSID_CORE_31: u32 = 0x33310000;

// DWC3 Global Control Register (GCTL) bits
pub const DWC3_GCTL_PWRDNSCALE_MASK: u32 = 0xFFF80000;
pub const DWC3_GCTL_PWRDNSCALE_SHIFT: u32 = 19;
pub const DWC3_GCTL_U2RSTECN: u32 = 1 << 16;
pub const DWC3_GCTL_RAMCLKSEL_MASK: u32 = 3 << 6;
pub const DWC3_GCTL_PRTCAPDIR_MASK: u32 = 3 << 12;
pub const DWC3_GCTL_PRTCAPDIR_HOST: u32 = 1 << 12;
pub const DWC3_GCTL_PRTCAPDIR_DEVICE: u32 = 2 << 12;
pub const DWC3_GCTL_PRTCAPDIR_OTG: u32 = 3 << 12;
pub const DWC3_GCTL_CORESOFTRESET: u32 = 1 << 11;
pub const DWC3_GCTL_SCALEDOWN_MASK: u32 = 3 << 4;
pub const DWC3_GCTL_DISSCRAMBLE: u32 = 1 << 3;
pub const DWC3_GCTL_DSBLCLKGTNG: u32 = 1 << 0;

// DWC3 Global Hardware Params 1 (GHWPARAMS1)
pub const DWC3_GHWPARAMS1_EN_PWROPT_MASK: u32 = 3 << 24;
pub const DWC3_GHWPARAMS1_EN_PWROPT_NO: u32 = 0;
pub const DWC3_GHWPARAMS1_EN_PWROPT_CLK: u32 = 1 << 24;

// DWC3 USB2 PHY Configuration Register (GUSB2PHYCFG) bits
pub const DWC3_GUSB2PHYCFG_PHYSOFTRST: u32 = 1 << 31;
pub const DWC3_GUSB2PHYCFG_U2_FREECLK_EXISTS: u32 = 1 << 30;
pub const DWC3_GUSB2PHYCFG_ENBLSLPM: u32 = 1 << 8;
pub const DWC3_GUSB2PHYCFG_SUSPHY: u32 = 1 << 6;
pub const DWC3_GUSB2PHYCFG_PHYIF: u32 = 1 << 3;
pub const DWC3_GUSB2PHYCFG_USBTRDTIM_MASK: u32 = 0xF << 10;
pub const DWC3_GUSB2PHYCFG_USBTRDTIM_16BIT: u32 = 0x5 << 10;
pub const DWC3_GUSB2PHYCFG_USBTRDTIM_8BIT: u32 = 0x9 << 10;

// DWC3 USB3 PIPE Control Register (GUSB3PIPECTL) bits
pub const DWC3_GUSB3PIPECTL_PHYSOFTRST: u32 = 1 << 31;
pub const DWC3_GUSB3PIPECTL_DISRXDETP3: u32 = 1 << 28;
pub const DWC3_GUSB3PIPECTL_SUSPHY: u32 = 1 << 17;

// DWC3 revision mask
pub const DWC3_REVISION_MASK: u32 = 0xFFFF;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dwc3Mode {
    Host,
    Device,
    Otg,
}

#[derive(Debug, Clone, Default)]
pub struct Dwc3Quirks {
    pub dis_enblslpm: bool,
    pub dis_u2_freeclk_exists: bool,
    pub dis_u2_susphy: bool,
    pub utmi_wide: bool,
}

impl Dwc3Quirks {
    pub fn rk3588_default() -> Self {
        Self {
            dis_enblslpm: true,
            dis_u2_freeclk_exists: true,
            dis_u2_susphy: false,
            utmi_wide: true,
        }
    }
}

pub struct Dwc3 {
    regs: NonNull<Dwc3Regs>,
}

unsafe impl Send for Dwc3 {}

impl Dwc3 {
    /// Create a new DWC3 instance from XHCI base address
    /// 
    /// # Safety
    /// The caller must ensure the XHCI base address is valid and properly mapped.
    pub unsafe fn from_xhci_base(xhci_base: NonNull<u8>) -> Self {
        unsafe {
            let dwc3_addr = xhci_base.as_ptr().add(DWC3_REG_OFFSET);
            Self {
                regs: NonNull::new_unchecked(dwc3_addr as *mut Dwc3Regs),
            }
        }
    }

    /// Read a register
    fn read_reg(&self, offset: usize) -> u32 {
        unsafe {
            let ptr = (self.regs.as_ptr() as *const u8).add(offset) as *const u32;
            ptr.read_volatile()
        }
    }

    /// Write a register
    fn write_reg(&self, offset: usize, val: u32) {
        unsafe {
            let ptr = (self.regs.as_ptr() as *mut u8).add(offset) as *mut u32;
            ptr.write_volatile(val);
        }
    }

    /// Read GCTL register
    fn read_gctl(&self) -> u32 {
        self.read_reg(0x10)
    }

    /// Write GCTL register
    fn write_gctl(&self, val: u32) {
        self.write_reg(0x10, val);
    }

    /// Read GUSB2PHYCFG[0] register  
    fn read_gusb2phycfg(&self) -> u32 {
        self.read_reg(0x100)
    }

    /// Write GUSB2PHYCFG[0] register
    fn write_gusb2phycfg(&self, val: u32) {
        self.write_reg(0x100, val);
    }

    /// Read GUSB3PIPECTL[0] register
    fn read_gusb3pipectl(&self) -> u32 {
        self.read_reg(0x1C0)
    }

    /// Write GUSB3PIPECTL[0] register
    fn write_gusb3pipectl(&self, val: u32) {
        self.write_reg(0x1C0, val);
    }

    /// Read GHWPARAMS1 register
    fn read_ghwparams1(&self) -> u32 {
        self.read_reg(0x44)
    }

    /// Read Synopsys ID register
    fn read_gsnpsid(&self) -> u32 {
        self.read_reg(0x20)
    }

    /// Verify this is a valid DWC3 core
    pub fn verify_id(&self) -> bool {
        let id = self.read_gsnpsid();
        let masked = id & DWC3_GSNPSID_MASK;
        
        if masked == DWC3_GSNPSID_CORE_3 || masked == DWC3_GSNPSID_CORE_31 {
            let revision = id & DWC3_REVISION_MASK;
            debug!("DWC3 Core ID: {:#010x}, revision: {:#06x}", id, revision);
            true
        } else {
            warn!("Not a DWC3 core: ID={:#010x}", id);
            false
        }
    }

    /// Perform PHY soft reset sequence
    fn phy_reset(&self) {
        debug!("DWC3: PHY reset sequence");
        
        // NOTE: Skip USB3 PHY soft reset - this causes the SuperSpeed port
        // to become invisible to xHCI on RK3588 when external USBDP PHY is used.
        // The USBDP PHY is already initialized before DWC3 init.
        // Only reset USB2 PHY.

        // Assert USB2 PHY reset
        let mut phycfg = self.read_gusb2phycfg();
        phycfg |= DWC3_GUSB2PHYCFG_PHYSOFTRST;
        self.write_gusb2phycfg(phycfg);

        delay_ms(100);

        // Clear USB2 PHY reset
        phycfg = self.read_gusb2phycfg();
        phycfg &= !DWC3_GUSB2PHYCFG_PHYSOFTRST;
        self.write_gusb2phycfg(phycfg);

        debug!("DWC3: PHY reset complete (USB2 only)");
    }

    /// Perform core soft reset
    fn core_soft_reset(&self) {
        debug!("DWC3: Core soft reset");

        // Put core in reset before resetting PHY
        let mut gctl = self.read_gctl();
        gctl |= DWC3_GCTL_CORESOFTRESET;
        self.write_gctl(gctl);

        // Reset USB3 and USB2 PHY
        self.phy_reset();

        // Wait 100ms for core reset
        delay_ms(100);

        // Take core out of reset
        gctl = self.read_gctl();
        gctl &= !DWC3_GCTL_CORESOFTRESET;
        self.write_gctl(gctl);

        debug!("DWC3: Core soft reset complete");
    }

    /// Initialize DWC3 core
    /// 
    /// This follows the U-Boot dwc3_core_init sequence:
    /// 1. Verify DWC3 ID
    /// 2. Perform core soft reset  
    /// 3. Configure power optimization
    /// 4. Apply revision-specific workarounds
    pub fn core_init(&self) -> Result<(), &'static str> {
        if !self.verify_id() {
            return Err("Not a DWC3 core");
        }

        // NOTE: Skip core soft reset on RK3588 with external USBDP PHY.
        // The core soft reset causes the SuperSpeed port to become invisible
        // to xHCI when the USBDP PHY has already been initialized.
        // self.core_soft_reset();

        let hwparams1 = self.read_ghwparams1();
        
        let mut gctl = self.read_gctl();
        
        gctl &= !DWC3_GCTL_SCALEDOWN_MASK;
        gctl &= !DWC3_GCTL_DISSCRAMBLE;

        let pwropt = (hwparams1 & DWC3_GHWPARAMS1_EN_PWROPT_MASK) >> 24;
        if pwropt == 1 {
            gctl &= !DWC3_GCTL_DSBLCLKGTNG;
            debug!("DWC3: Clock gating enabled");
        }

        let revision = self.read_gsnpsid() & DWC3_REVISION_MASK;
        if revision < 0x190a {
            gctl |= DWC3_GCTL_U2RSTECN;
            debug!("DWC3: Applied U2RSTECN workaround for revision {:#x}", revision);
        }

        self.write_gctl(gctl);

        debug!("DWC3: Core initialized");
        Ok(())
    }

    /// Configure USB2 PHY settings with quirks
    pub fn configure_usb2_phy(&self, quirks: &Dwc3Quirks) {
        let mut reg = self.read_gusb2phycfg();

        // Configure UTMI interface width
        if quirks.utmi_wide {
            reg |= DWC3_GUSB2PHYCFG_PHYIF;
            reg &= !DWC3_GUSB2PHYCFG_USBTRDTIM_MASK;
            reg |= DWC3_GUSB2PHYCFG_USBTRDTIM_16BIT;
            debug!("DWC3: USB2 PHY configured for UTMI wide (16-bit)");
        }

        // Apply quirks
        if quirks.dis_enblslpm {
            reg &= !DWC3_GUSB2PHYCFG_ENBLSLPM;
            debug!("DWC3: Disabled ENBLSLPM");
        }

        if quirks.dis_u2_freeclk_exists {
            reg &= !DWC3_GUSB2PHYCFG_U2_FREECLK_EXISTS;
            debug!("DWC3: Disabled U2_FREECLK_EXISTS");
        }

        if quirks.dis_u2_susphy {
            reg &= !DWC3_GUSB2PHYCFG_SUSPHY;
            debug!("DWC3: Disabled U2 SUSPHY");
        }

        self.write_gusb2phycfg(reg);
    }

    /// Set DWC3 operating mode (Host/Device/OTG)
    pub fn set_mode(&self, mode: Dwc3Mode) {
        let mut gctl = self.read_gctl();
        gctl &= !DWC3_GCTL_PRTCAPDIR_MASK;
        
        match mode {
            Dwc3Mode::Host => {
                gctl |= DWC3_GCTL_PRTCAPDIR_HOST;
                debug!("DWC3: Set to Host mode");
            }
            Dwc3Mode::Device => {
                gctl |= DWC3_GCTL_PRTCAPDIR_DEVICE;
                debug!("DWC3: Set to Device mode");
            }
            Dwc3Mode::Otg => {
                gctl |= DWC3_GCTL_PRTCAPDIR_OTG;
                debug!("DWC3: Set to OTG mode");
            }
        }
        
        self.write_gctl(gctl);
    }

    /// Full initialization sequence for XHCI host mode
    /// 
    /// This performs:
    /// 1. Core initialization (soft reset, config)
    /// 2. USB2 PHY configuration with quirks
    /// 3. Set host mode
    pub fn init_for_xhci_host(&self, quirks: &Dwc3Quirks) -> Result<(), &'static str> {
        // Initialize core
        self.core_init()?;

        // Configure USB2 PHY
        self.configure_usb2_phy(quirks);

        // Set host mode
        self.set_mode(Dwc3Mode::Host);

        debug!("DWC3: Initialized for XHCI host mode");
        Ok(())
    }
}

pub unsafe fn init_dwc3_for_xhci(xhci_base: NonNull<u8>) -> Result<(), &'static str> {
    let dwc3 = unsafe { Dwc3::from_xhci_base(xhci_base) };
    let quirks = Dwc3Quirks::rk3588_default();
    dwc3.init_for_xhci_host(&quirks)
}

pub unsafe fn is_dwc3_xhci(xhci_base: NonNull<u8>) -> bool {
    let dwc3 = unsafe { Dwc3::from_xhci_base(xhci_base) };
    dwc3.verify_id()
}

pub unsafe fn read_gctl(xhci_base: NonNull<u8>) -> u32 {
    let dwc3 = unsafe { Dwc3::from_xhci_base(xhci_base) };
    dwc3.read_reg(0x10)
}

pub unsafe fn read_gusb2phycfg(xhci_base: NonNull<u8>) -> u32 {
    let dwc3 = unsafe { Dwc3::from_xhci_base(xhci_base) };
    dwc3.read_reg(0x100)
}

pub unsafe fn read_gusb3pipectl(xhci_base: NonNull<u8>) -> u32 {
    let dwc3 = unsafe { Dwc3::from_xhci_base(xhci_base) };
    dwc3.read_reg(0x1C0)
}
