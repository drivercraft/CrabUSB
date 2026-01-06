//! DWC3 (DesignWare USB3 Controller) 驱动
//!
//! DWC3 是一个 USB3 DRD (Dual Role Device) 控制器，支持 Host 和 Device 模式。
//! 本模块实现 Host 模式驱动，基于 xHCI 规范。

use core::ops::{Deref, DerefMut};

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use tock_registers::interfaces::*;
use usb_if::DeviceSpeed;
pub use usb_if::DrMode;

use crate::{
    Mmio, Xhci,
    backend::{
        dwc::{
            event::EventBuffer,
            reg::{GCTL, GHWPARAMS1, GHWPARAMS4, GUSB3PIPECTL},
            udphy::Udphy,
        },
        ty::HostOp,
    },
    err::{Result, USBError},
};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

pub mod grf;
// pub mod phy;
mod consts;
mod event;
mod reg;
mod udphy;
pub mod usb2phy;

// pub use phy::{UsbDpMode, UsbDpPhy, UsbDpPhyConfig};
use consts::*;
use reg::Dwc3Regs;
pub use udphy::UdphyParam;
pub use usb2phy::Usb2Phy;

/// CRU (Clock and Reset Unit)
pub trait CruOp: Sync + Send + 'static {
    fn reset_assert(&self, id: u64);
    fn reset_deassert(&self, id: u64);
}

pub struct DwcNewParams<'a, C: CruOp> {
    pub ctrl: Mmio,
    pub phy: Mmio,
    pub phy_param: UdphyParam<'a>,
    pub cru: C,
    pub rst_list: &'a [(&'a str, u64)],
    pub dma_mask: usize,
    pub params: DwcParams,
}

#[derive(Debug, Default, Clone)]
pub struct DwcParams {
    pub dr_mode: DrMode,
    pub max_speed: DeviceSpeed,
    pub delayed_status: bool,
    pub ep0_bounced: bool,
    pub ep0_expect_in: bool,
    pub has_hibernation: bool,
    pub has_lpm_erratum: bool,
    pub is_utmi_l1_suspend: bool,
    pub is_selfpowered: bool,
    pub is_fpga: bool,
    pub needs_fifo_resize: bool,
    pub pullups_connected: bool,
    pub resize_fifos: bool,
    pub setup_packet_pending: bool,
    pub start_config_issued: bool,
    pub three_stage_setup: bool,
    pub disable_scramble_quirk: bool,
    pub u2exit_lfps_quirk: bool,
    pub u2ss_inp3_quirk: bool,
    pub req_p1p2p3_quirk: bool,
    pub del_p1p2p3_quirk: bool,
    pub del_phy_power_chg_quirk: bool,
    pub lfps_filter_quirk: bool,
    pub rx_detect_poll_quirk: bool,
    pub dis_u3_susphy_quirk: bool,
    pub dis_u2_susphy_quirk: bool,
    pub dis_u1u2_quirk: bool,
    pub dis_enblslpm_quirk: bool,
    pub dis_u2_freeclk_exists_quirk: bool,
    pub tx_de_emphasis_quirk: bool,
    pub tx_de_emphasis: u8,        // 2 bits
    pub usb2_phyif_utmi_width: u8, // 5 bits
}

/// DWC3 控制器
///
/// DWC3 实际上是 xHCI 主机控制器的封装。在 Host 模式下，
/// DWC3 的 xHCI 寄存器区域 (0x0000 - 0x7fff) 包含标准 xHCI 寄存器，
/// 全局寄存器区域 (0xc100 - 0xcfff) 包含 DWC3 特定配置。
pub struct Dwc {
    xhci: Xhci,
    phy: Udphy,
    // usb2_phy: Usb2Phy,
    dwc_regs: Dwc3Regs,
    cru: Arc<dyn CruOp>,
    rsts: BTreeMap<String, u64>,
    ev_buffs: Vec<EventBuffer>,
    revistion: u32,
    nr_scratch: u32,
    params: DwcParams,
}

impl Dwc {
    pub fn new(params: DwcNewParams<'_, impl CruOp>) -> Result<Self> {
        let mmio_base = params.ctrl.as_ptr() as usize;
        let cru = Arc::new(params.cru);

        let phy = Udphy::new(params.phy, cru.clone(), params.phy_param);

        let xhci = Xhci::new(params.ctrl, params.dma_mask)?;

        let dwc_regs = unsafe { Dwc3Regs::new(mmio_base) };

        let mut rsts = BTreeMap::new();
        for &(name, id) in params.rst_list.iter() {
            rsts.insert(String::from(name), id);
        }

        Ok(Self {
            xhci,
            dwc_regs,
            phy,
            cru,
            rsts,
            ev_buffs: vec![],
            revistion: 0,
            nr_scratch: 0,
            params: params.params,
            // usb2_phy,
        })
    }

    async fn dwc3_init(&mut self) -> Result<()> {
        self.alloc_event_buffers(DWC3_EVENT_BUFFERS_SIZE)?;
        self.core_init().await?;

        Ok(())
    }

    fn alloc_event_buffers(&mut self, len: usize) -> Result<()> {
        let num_buffs = self.dwc_regs.num_event_buffers();
        debug!("Allocating {} event buffers", num_buffs);
        for _ in 0..num_buffs {
            let ev_buff = EventBuffer::new(len, self.xhci.dma_mask)?;
            self.ev_buffs.push(ev_buff);
        }
        Ok(())
    }

    async fn core_init(&mut self) -> Result<()> {
        self.revistion = self.dwc_regs.read_revision() as _;
        if self.revistion != 0x55330000 {
            return Err(USBError::Other(format!(
                "Unsupported DWC3 revision: 0x{:08x}",
                self.revistion
            )));
        }
        self.revistion += self.dwc_regs.read_product_id();
        debug!("DWC3: Detected revision 0x{:08x}", self.revistion);

        self.dwc_regs.device_soft_reset().await;
        self.dwc_regs.core_soft_reset().await;
        if self.revistion >= DWC3_REVISION_250A {
            debug!("DWC3: Revision 250A or later detected");
        }

        let mut reg = self.dwc_regs.globals().gctl.extract();
        reg.modify(GCTL::SCALEDOWN::None);

        match self
            .dwc_regs
            .globals()
            .ghwparams1
            .read_as_enum(GHWPARAMS1::EN_PWROPT)
        {
            Some(GHWPARAMS1::EN_PWROPT::Value::Clock) => {
                if (DWC3_REVISION_210A..=DWC3_REVISION_250A).contains(&self.revistion) {
                    reg.modify(GCTL::DSBLCLKGTNG::Enable + GCTL::SOFITPSYNC::Enable);
                } else {
                    reg.modify(GCTL::DSBLCLKGTNG::Disable);
                }
            }
            Some(GHWPARAMS1::EN_PWROPT::Value::Hibernation) => {
                self.nr_scratch = self
                    .dwc_regs
                    .globals()
                    .ghwparams4
                    .read(GHWPARAMS4::HIBER_SCRATCHBUFS) as _;

                reg.modify(GCTL::GBLHIBERNATIONEN::Enable);
            }
            _ => {
                debug!("No power optimization available");
            }
        }

        /*
         * WORKAROUND: DWC3 revisions <1.90a have a bug
         * where the device can fail to connect at SuperSpeed
         * and falls back to high-speed mode which causes
         * the device to enter a Connect/Disconnect loop
         */
        if self.revistion < DWC3_REVISION_190A {
            debug!("Applying DWC3 <1.90a SuperSpeed connect workaround");
            reg.modify(GCTL::U2RSTECN::Enable);
        }

        // core_num_eps

        self.dwc_regs.globals().gctl.set(reg.get());

        self.phy_setup().await?;

        Ok(())
    }

    async fn phy_setup(&mut self) -> Result<()> {
        let mut reg = self.dwc_regs.globals().gusb3pipectl0.extract();

        if self.revistion >= DWC3_REVISION_194A {
            reg.modify(GUSB3PIPECTL::SUSPHY::Enable);
        }

        Ok(())
    }
}

impl HostOp for Dwc {
    type DeviceInfo = DeviceInfo;
    type EventHandler = EventHandler;

    /// 初始化 DWC3 控制器
    ///
    /// ## 初始化顺序说明
    ///
    /// 在 HOST 模式下，必须按照以下顺序初始化：
    /// 1. USBDP PHY 硬件初始化（时钟、复位、PLL）
    /// 2. DWC3 全局配置（GCTL、HOST 模式）
    /// 3. **xHCI 主机控制器初始化**（执行 HCRST 复位）
    /// 4. DWC3 PHY 配置寄存器（GUSB3PIPECTL、GUSB2PHYCFG）
    ///
    /// **关键点**：DWC3 PHY 配置寄存器必须在 xHCI 执行 HCRST **之后**才能访问，
    /// 因为 HCRST 会复位并使能 host block 的 PHY 接口。
    async fn init(&mut self) -> Result {
        info!("DWC3: Starting controller initialization");

        /*
         * It must hold whole USB3.0 OTG controller in resetting to hold pipe
         * power state in P2 before initializing TypeC PHY on RK3399 platform.
         */
        for &id in self.rsts.values() {
            self.cru.reset_assert(id);
        }

        crate::osal::kernel::delay(core::time::Duration::from_millis(1));

        // 步骤 2: 配置 USBDP PHY 硬件（时钟、复位、PLL 等）
        info!("DWC3: Step 2 - Configuring USBDP PHY hardware");

        self.phy.init().await?;

        for &id in self.rsts.values() {
            self.cru.reset_deassert(id);
        }

        self.dwc3_init().await?;

        Ok(())
    }

    /// 探测 USB 设备
    async fn probe_devices(&mut self) -> Result<Vec<Self::DeviceInfo>> {
        let devices = self.xhci.probe_devices().await?;
        Ok(devices)
    }

    /// 打开 USB 设备
    async fn open_device(
        &mut self,
        dev: &Self::DeviceInfo,
    ) -> Result<<Self::DeviceInfo as super::ty::DeviceInfoOp>::Device> {
        let device = self.xhci.open_device(dev).await?;
        Ok(device)
    }

    /// 创建事件处理器
    fn create_event_handler(&mut self) -> Self::EventHandler {
        self.xhci.create_event_handler()
    }
}

impl Deref for Dwc {
    type Target = DwcParams;

    fn deref(&self) -> &Self::Target {
        &self.params
    }
}

impl DerefMut for Dwc {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.params
    }
}
