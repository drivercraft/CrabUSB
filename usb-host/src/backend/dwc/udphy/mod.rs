use crate::{
    Mmio,
    backend::dwc::udphy::regmap::{
        RK3588_UDPHY_24M_REFCLK_CFG, RK3588_UDPHY_INIT_SEQUENCE, Regmap,
    },
    err::Result,
};

mod config;
mod consts;
mod regmap;

use consts::*;

/// USBDP PHY 模式
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum UsbDpMode {
    None = 0,
    Usb = 1,
    Dp = 2,
    UsbDp = 3,
}

/// USBDP PHY 寄存器偏移
pub const UDPHY_PMA: usize = 0x8000;
pub const UDPHY_PCS: usize = 0x4000;

pub struct Udphy {
    cfg: config::UdphyCfg,
    mode: UsbDpMode,
    /// PHY MMIO 基址
    phy_base: usize,

    pma_remap: Regmap,
    /// USBDP PHY GRF
    udphygrf: Regmap,
    // /// USB GRF
    // usb_grf: Grf,
    // /// USB2PHY GRF
    // usb2phy_grf: Grf,
    lane_mux_sel: [u32; 4],
}

impl Udphy {
    pub fn new(base: Mmio, usb_grf: Mmio, dp_grf: Mmio, usb2phy_grf: Mmio) -> Self {
        let cfg = config::RK3588_UDPHY_CFGS.clone();

        Udphy {
            cfg,
            mode: UsbDpMode::Usb,
            phy_base: base.as_ptr() as usize,
            pma_remap: Regmap::new(unsafe { base.add(UDPHY_PMA) }),
            udphygrf: Regmap::new(dp_grf),
            lane_mux_sel: [0; 4],
        }
    }

    pub async fn init(&self) -> Result<()> {
        info!("Starting initialization");

        // enable rx lfps for usb
        if matches!(self.mode, UsbDpMode::Usb | UsbDpMode::UsbDp) {
            debug!("Enabling RX LFPS for USB mode");
            self.udphygrf.grfreg_write(&self.cfg.grf.rx_lfps, true);
        }

        // Step 1: power on pma and deassert apb rstn
        self.udphygrf.grfreg_write(&self.cfg.grf.low_pwrn, true);

        self.pma_remap.multi_reg_write(RK3588_UDPHY_INIT_SEQUENCE);

        self.pma_remap.multi_reg_write(RK3588_UDPHY_24M_REFCLK_CFG);

        // Step 3: configure lane mux
        self.pma_remap.update_bits(
            CMN_LANE_MUX_AND_EN_OFFSET,
            CMN_DP_LANE_MUX_ALL | CMN_DP_LANE_EN_ALL,
            FIELD_PREP(CMN_DP_LANE_MUX_N(3), self.lane_mux_sel[3])
                | FIELD_PREP(CMN_DP_LANE_MUX_N(2), self.lane_mux_sel[2])
                | FIELD_PREP(CMN_DP_LANE_MUX_N(1), self.lane_mux_sel[1])
                | FIELD_PREP(CMN_DP_LANE_MUX_N(0), self.lane_mux_sel[0])
                | FIELD_PREP(CMN_DP_LANE_EN_ALL, 0),
        );

        Ok(())
    }
}
