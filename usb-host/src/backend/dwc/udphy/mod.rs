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
use tock_registers::{
    interfaces::{ReadWriteable, Writeable},
    registers::ReadWrite,
};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UdphyMode: u8 {
        const NONE = 0;
        const USB = 1;
        const DP = 1 << 1;
        const DP_USB = Self::DP.bits | Self::USB.bits;
    }
}

/// USBDP PHY 寄存器偏移
pub const UDPHY_PMA: usize = 0x8000;
pub const UDPHY_PCS: usize = 0x4000;

pub struct UdphyParam<'a> {
    /// prop `rockchip,usb2phy-grf`
    pub u2phy_grf: Mmio,
    /// prop `rockchip,usb-grf`
    pub usb_grf: Mmio,
    /// prop `rockchip,usbdpphy-grf`
    pub usbdpphy_grf: Mmio,
    /// prop `rockchip,vo-grf`
    pub vo_grf: Mmio,
    /// prop `rockchip,dp-lane-mux`
    pub dp_lane_mux: &'a [u32],
}

pub struct Udphy {
    cfg: config::UdphyCfg,
    mode: UdphyMode,
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
    dp_lane_sel: [u32; 4],
}

impl Udphy {
    pub fn new(base: Mmio, param: UdphyParam<'_>) -> Self {
        let cfg = config::RK3588_UDPHY_CFGS.clone();
        let mut lane_mux_sel = [0u32; 4];
        let mut dp_lane_sel = [0u32; 4];
        for (i, &lane) in param.dp_lane_mux.iter().enumerate() {
            debug!("DP lane {} mux select: {}", i, lane);
            dp_lane_sel[i] = lane;
            if lane > 3 {
                panic!("lane mux between 0 and 3, exceeding the range");
            }
            lane_mux_sel[lane as usize] = PHY_LANE_MUX_DP;

            for j in 0..param.dp_lane_mux.len() {
                if lane == dp_lane_sel[j] {
                    panic!("set repeat lane mux value")
                }
            }
        }

        let mut mode = UdphyMode::DP;

        if param.dp_lane_mux.len() == 2 {
            mode |= UdphyMode::USB;
        }

        Udphy {
            cfg,
            mode,
            phy_base: base.as_ptr() as usize,
            pma_remap: Regmap::new(unsafe { base.add(UDPHY_PMA) }),
            udphygrf: Regmap::new(param.usbdpphy_grf),
            lane_mux_sel,
            dp_lane_sel,
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
        self.cmn_lane_mux_and_en().write(
            CMN_LANE_MUX_EN::LANE0_MUX.val(self.lane_mux_sel[0])
                + CMN_LANE_MUX_EN::LANE1_MUX.val(self.lane_mux_sel[1])
                + CMN_LANE_MUX_EN::LANE2_MUX.val(self.lane_mux_sel[2])
                + CMN_LANE_MUX_EN::LANE3_MUX.val(self.lane_mux_sel[3])
                + CMN_LANE_MUX_EN::LANE0_EN::Disable
                + CMN_LANE_MUX_EN::LANE1_EN::Disable
                + CMN_LANE_MUX_EN::LANE2_EN::Disable
                + CMN_LANE_MUX_EN::LANE3_EN::Disable,
        );
        // Step 4: deassert init rstn and wait for 200ns from datasheet
        

        Ok(())
    }

    fn cmn_lane_mux_and_en(&self) -> &ReadWrite<u32, CMN_LANE_MUX_EN::Register> {
        unsafe { &*((self.phy_base + UDPHY_PMA + pma_offset::CMN_LANE_MUX_AND_EN) as *const _) }
    }
}
