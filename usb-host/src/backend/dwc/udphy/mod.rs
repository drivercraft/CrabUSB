use core::time::Duration;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use super::CruOp;
use crate::{
    Mmio,
    backend::dwc::udphy::regmap::{
        RK3588_UDPHY_24M_REFCLK_CFG, RK3588_UDPHY_INIT_SEQUENCE, Regmap,
    },
    err::Result,
    osal::SpinWhile,
};

mod config;
mod consts;
mod regmap;

use consts::*;
use tock_registers::{interfaces::*, registers::*};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UdphyMode: u8 {
        const NONE = 0;
        const USB = 1;
        const DP = 1 << 1;
        const DP_USB = Self::DP.bits() | Self::USB.bits();
    }
}

/// USBDP PHY 寄存器偏移
pub const UDPHY_PMA: usize = 0x8000;
pub const UDPHY_PCS: usize = 0x4000;

pub struct UdphyParam<'a> {
    pub id: usize,
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
    pub rst_list: &'a [(&'a str, u64)],
}

pub struct Udphy {
    id: usize,
    cfg: Box<config::UdphyCfg>,
    mode: UdphyMode,
    /// PHY MMIO 基址
    phy_base: usize,

    pma_remap: Regmap,
    /// USBDP PHY GRF
    udphygrf: Regmap,
    /// USB GRF
    usb_grf: Regmap,
    // /// USB2PHY GRF
    // usb2phy_grf: Grf,
    lane_mux_sel: [u32; 4],
    dp_lane_sel: [u32; 4],
    /// Type C 反转标志
    flip: bool,
    cru: Arc<dyn CruOp>,
    rsts: BTreeMap<String, u64>,
}

impl Udphy {
    pub fn new(base: Mmio, cru: Arc<dyn CruOp>, param: UdphyParam<'_>) -> Self {
        let cfg = Box::new(config::RK3588_UDPHY_CFGS.clone());
        let mut lane_mux_sel = [0u32; 4];
        let mut dp_lane_sel = [0u32; 4];
        let num_lanes = param.dp_lane_mux.len();

        if num_lanes != 2 && num_lanes != 4 {
            panic!("dp_lane_mux length must be 2 or 4");
        }

        for (i, &lane) in param.dp_lane_mux.iter().enumerate() {
            debug!("DP lane {} mux select: {}", i, lane);
            dp_lane_sel[i] = lane;
            if lane > 3 {
                panic!("lane mux between 0 and 3, exceeding the range");
            }
            lane_mux_sel[lane as usize] = PHY_LANE_MUX_DP;

            let mut j = i + 1;
            while j < num_lanes {
                if dp_lane_sel[j] == lane {
                    panic!("duplicate lane mux selection for lane {}", lane);
                }
                j += 1;
            }
        }

        let mut mode = UdphyMode::DP;
        let mut flip = false;

        if param.dp_lane_mux.len() == 2 {
            mode |= UdphyMode::USB;
            flip = lane_mux_sel[0] == PHY_LANE_MUX_DP;
        }

        let mut rsts = BTreeMap::new();
        for &(name, id) in param.rst_list.iter() {
            if cfg.rst_list.contains(&name) {
                rsts.insert(String::from(name), id);
            } else {
                panic!("unsupported reset name: {}", name);
            }
        }

        Udphy {
            id: param.id,
            cfg,
            mode,
            phy_base: base.as_ptr() as usize,
            pma_remap: Regmap::new(unsafe { base.add(UDPHY_PMA) }),
            udphygrf: Regmap::new(param.usbdpphy_grf),
            usb_grf: Regmap::new(param.usb_grf),
            lane_mux_sel,
            dp_lane_sel,
            cru,
            rsts,
            flip,
        }
    }

    pub async fn setup(&mut self) -> Result<()> {
        info!("Starting initialization");
        for &rst in self.cfg.rst_list {
            self.reset_assert(rst);
        }

        // enable rx lfps for usb
        if self.mode.contains(UdphyMode::USB) {
            debug!("Enabling RX LFPS for USB mode");
            self.udphygrf.grfreg_write(&self.cfg.grf.rx_lfps, true);
        }

        // Step 1: power on pma and deassert apb rstn
        self.udphygrf.grfreg_write(&self.cfg.grf.low_pwrn, true);

        self.reset_deassert("pma_apb");
        self.reset_deassert("pcs_apb");
        debug!("PMA powered on and APB resets deasserted");

        // Step 2: set init sequence and phy refclk
        self.pma_remap.multi_reg_write(RK3588_UDPHY_INIT_SEQUENCE);

        debug!("Initial register sequences applied");

        self.pma_remap.multi_reg_write(RK3588_UDPHY_24M_REFCLK_CFG);

        debug!("24M reference clock configured");

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
        if self.mode.contains(UdphyMode::USB) {
            self.reset_deassert("init");
        }

        if self.mode.contains(UdphyMode::DP) {
            self.cmn_dp_rstn().modify(CMN_DP_RSTN::DP_INIT_RSTN::Enable);
        }

        crate::osal::kernel::delay(Duration::from_micros(1));

        if self.mode.contains(UdphyMode::USB) {
            // Step 5: deassert usb rstn
            self.reset_deassert("cmn");
            self.reset_deassert("lane");
        }
        //  Step 6: wait for lock done of pll
        self.status_check().await;
        info!("Udphy initialized");

        self.u3_port_disable(!self.mode.contains(UdphyMode::USB));

        let dplanes = self.dplane_get();
        debug!(
            "Configured for {:?} mode with {} DP lanes",
            self.mode, dplanes
        );
        self.dplane_enable(dplanes);

        Ok(())
    }

    fn dplane_enable(&self, lanes: usize) {
        match lanes {
            4 => {
                self.cmn_lane_mux_and_en().modify(
                    CMN_LANE_MUX_EN::LANE0_EN::Enable
                        + CMN_LANE_MUX_EN::LANE1_EN::Enable
                        + CMN_LANE_MUX_EN::LANE2_EN::Enable
                        + CMN_LANE_MUX_EN::LANE3_EN::Enable,
                );
            }
            2 => {
                self.cmn_lane_mux_and_en()
                    .modify(CMN_LANE_MUX_EN::LANE0_EN::Enable + CMN_LANE_MUX_EN::LANE1_EN::Enable);
            }
            0 => {
                self.cmn_dp_rstn().modify(CMN_DP_RSTN::DP_CMN_RSTN::CLEAR);
            }
            _ => {
                panic!("unsupported dplane lanes: {}", lanes);
            }
        }
    }

    fn dplane_get(&self) -> usize {
        match self.mode {
            UdphyMode::DP => 4,
            UdphyMode::DP_USB => 2,
            _ => 0,
        }
    }

    async fn status_check(&self) {
        if self.mode.contains(UdphyMode::USB) {
            debug!("Waiting for PLL lock...");
            SpinWhile::new(|| {
                !self.cmn_ana_lcpll().is_set(CMN_ANA_LCPLL::AFC_DONE)
                    || !self.cmn_ana_lcpll().is_set(CMN_ANA_LCPLL::LOCK_DONE)
            })
            .await;

            if self.flip {
                SpinWhile::new(|| {
                    !self
                        .trsv_ln2_mon_rx_cdr()
                        .is_set(TRSV_LN2_MON_RX_CDR::LOCK_DONE)
                })
                .await;
            } else {
                SpinWhile::new(|| {
                    !self
                        .trsv_ln0_mon_rx_cdr()
                        .is_set(TRSV_LN0_MON_RX_CDR::LOCK_DONE)
                })
                .await;
            }
        }
    }

    pub fn u3_port_disable(&self, disable: bool) {
        debug!("udphy{}: u3 port set disable: {disable}", self.id);

        let cfg = if self.id > 0 {
            &self.cfg.grf.usb3otg1_cfg
        } else {
            &self.cfg.grf.usb3otg0_cfg
        };

        self.usb_grf.grfreg_write(cfg, disable);
    }

    fn cmn_lane_mux_and_en(&self) -> &ReadWrite<u32, CMN_LANE_MUX_EN::Register> {
        unsafe { &*((self.phy_base + UDPHY_PMA + pma_offset::CMN_LANE_MUX_AND_EN) as *const _) }
    }

    fn cmn_dp_rstn(&self) -> &ReadWrite<u32, CMN_DP_RSTN::Register> {
        unsafe { &*((self.phy_base + UDPHY_PMA + pma_offset::CMN_DP_RSTN) as *const _) }
    }

    fn cmn_ana_lcpll(&self) -> &ReadWrite<u32, CMN_ANA_LCPLL::Register> {
        unsafe { &*((self.phy_base + UDPHY_PMA + pma_offset::CMN_ANA_LCPLL_DONE) as *const _) }
    }

    fn trsv_ln0_mon_rx_cdr(&self) -> &ReadOnly<u32, TRSV_LN0_MON_RX_CDR::Register> {
        unsafe { &*((self.phy_base + UDPHY_PMA + pma_offset::TRSV_LN0_MON_RX_CDR) as *const _) }
    }

    fn trsv_ln2_mon_rx_cdr(&self) -> &ReadOnly<u32, TRSV_LN2_MON_RX_CDR::Register> {
        unsafe { &*((self.phy_base + UDPHY_PMA + pma_offset::TRSV_LN2_MON_RX_CDR) as *const _) }
    }

    fn reset_assert(&self, name: &str) {
        if let Some(&rst_id) = self.rsts.get(name) {
            self.cru.reset_assert(rst_id);
        } else {
            panic!("unsupported reset name: {}", name);
        }
    }

    fn reset_deassert(&self, name: &str) {
        if let Some(&rst_id) = self.rsts.get(name) {
            self.cru.reset_deassert(rst_id);
        } else {
            panic!("unsupported reset name: {}", name);
        }
    }
}
