//! DWC3 (DesignWare USB3 Controller) 驱动
//!
//! DWC3 是一个 USB3 DRD (Dual Role Device) 控制器，支持 Host 和 Device 模式。
//! 本模块实现 Host 模式驱动，基于 xHCI 规范。

use core::ops::{Deref, DerefMut};

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use dma_api::DVec;
use tock_registers::interfaces::*;
use usb_if::DeviceSpeed;
pub use usb_if::DrMode;

use crate::{
    Mmio, Xhci,
    backend::{
        dwc::{
            event::EventBuffer,
            reg::{GCTL, GHWPARAMS1, GHWPARAMS3, GHWPARAMS4, GUCTL1},
            udphy::Udphy,
        },
        ty::HostOp,
    },
    err::{Result, USBError},
    osal::kernel::page_size,
};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

/// USB PHY 接口模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UsbPhyInterfaceMode {
    /// 未知模式
    #[default]
    Unknown,
    /// UTMI 8-bit 接口
    Utmi,
    /// UTMI 16-bit 接口 (UTMIW)
    UtmiWide,
}

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
// pub use usb2phy::Usb2Phy;

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
    pub hsphy_mode: UsbPhyInterfaceMode,
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
    scratchbuf: Option<DVec<u8>>,
}

impl Dwc {
    pub fn new(mut params: DwcNewParams<'_, impl CruOp>) -> Result<Self> {
        let mmio_base = params.ctrl.as_ptr() as usize;
        params.params.max_speed = DeviceSpeed::Super;
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
            scratchbuf: None,
            // usb2_phy,
        })
    }

    async fn dwc3_init(&mut self) -> Result<()> {
        self.alloc_event_buffers(DWC3_EVENT_BUFFERS_SIZE)?;
        self.core_init().await?;
        self.event_buffers_setup();

        Ok(())
    }

    fn alloc_event_buffers(&mut self, len: usize) -> Result<()> {
        let num_buffs = self
            .dwc_regs
            .globals()
            .ghwparams1
            .read(GHWPARAMS1::NUM_EVENT_BUFFERS);
        debug!("Allocating {} event buffers", num_buffs);
        for _ in 0..num_buffs {
            let ev_buff = EventBuffer::new(len, self.xhci.dma_mask)?;
            self.ev_buffs.push(ev_buff);
        }
        Ok(())
    }

    fn event_buffers_setup(&mut self) {
        use reg::GEVNTSIZ;

        info!("DWC3: Setting up event buffers");

        let regs = self.dwc_regs.globals();

        for (i, ev_buff) in self.ev_buffs.iter().enumerate() {
            if i >= regs.gevnt.len() {
                warn!("DWC3: Invalid event buffer index {}", i);
                break;
            }

            let dma_addr = ev_buff.dma_addr();
            let length = ev_buff.buffer.len();

            debug!(
                "DWC3: Event buffer {} - DMA addr: {:#x}, length: {}",
                i, dma_addr, length
            );

            // 使用 gevnt 数组访问事件缓冲区寄存器
            regs.gevnt[i].adrlo.set((dma_addr & 0xffffffff) as u32);
            regs.gevnt[i].adrhi.set((dma_addr >> 32) as u32);
            regs.gevnt[i].size.set(length as u32);
            regs.gevnt[i].count.set(0);
        }

        debug!("DWC3: Event buffers setup completed");
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

        if let Some(GHWPARAMS3::SSPHY_IFC::Value::Disabled) = self
            .dwc_regs
            .globals()
            .ghwparams3
            .read_as_enum(GHWPARAMS3::SSPHY_IFC)
            && self.max_speed == DeviceSpeed::Super
        {
            self.max_speed = DeviceSpeed::High;
        }

        debug!("DWC3: Max speed {:?}", self.max_speed);

        self.dwc_regs.device_soft_reset().await;
        self.dwc_regs.core_soft_reset().await;
        if self.revistion >= DWC3_REVISION_250A {
            debug!("DWC3: Revision 250A or later detected");

            if matches!(self.max_speed, DeviceSpeed::Full | DeviceSpeed::High) {
                self.dwc_regs
                    .globals()
                    .guctl1
                    .modify(GUCTL1::DEV_FORCE_20_CLK_FOR_30_CLK::Enable);
            }
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
        reg.modify(GCTL::DISSCRAMBLE::Disable);

        if self.u2exit_lfps_quirk {
            reg.modify(GCTL::U2EXIT_LFPS::Enable);
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

        self.alloc_scratch_buffers()?;

        self.setup_scratch_buffers();

        self.core_init_mode()?;

        Ok(())
    }

    /// 配置 USB2 High-Speed PHY 接口模式
    ///
    /// 根据 hsphy_mode 配置 PHY 接口：
    /// - Utmi: 8-bit UTMI 接口 (USBTRDTIM=9, PHYIF=0)
    /// - UtmiWide: 16-bit UTMI 接口 (USBTRDTIM=5, PHYIF=1)
    fn hsphy_mode_setup(&mut self) {
        use reg::GUSB2PHYCFG;

        match self.hsphy_mode {
            UsbPhyInterfaceMode::Utmi => {
                // 8-bit UTMI 接口
                self.dwc_regs.globals().gusb2phycfg0.modify(
                    GUSB2PHYCFG::PHYIF.val(0) + // UTMI_PHYIF_8_BIT
                    GUSB2PHYCFG::USBTRDTIM.val(9), // USBTRDTIM_UTMI_8_BIT
                );
                debug!("DWC3: HS PHY configured as UTMI 8-bit");
            }
            UsbPhyInterfaceMode::UtmiWide => {
                // 16-bit UTMI 接口
                self.dwc_regs.globals().gusb2phycfg0.modify(
                    GUSB2PHYCFG::PHYIF.val(1) + // UTMI_PHYIF_16_BIT
                    GUSB2PHYCFG::USBTRDTIM.val(5), // USBTRDTIM_UTMI_16_BIT
                );
                debug!("DWC3: HS PHY configured as UTMI 16-bit");
            }
            UsbPhyInterfaceMode::Unknown => {
                debug!("DWC3: HS PHY mode unknown, using default configuration");
            }
        }
    }

    async fn phy_setup(&mut self) -> Result<()> {
        use reg::{GUSB2PHYCFG, GUSB3PIPECTL};

        info!("DWC3: Configuring PHY");

        // === USB3 PHY 配置 ===
        let mut gusb3 = self.dwc_regs.globals().gusb3pipectl0.extract();

        /*
         * Above 1.94a, it is recommended to set DWC3_GUSB3PIPECTL_SUSPHY
         * to '0' during coreConsultant configuration. So default value
         * will be '0' when the core is reset. Application needs to set it
         * to '1' after the core initialization is completed.
         */
        if self.revistion > DWC3_REVISION_194A {
            gusb3.modify(GUSB3PIPECTL::SUSPHY::Enable);
        }

        if self.u2ss_inp3_quirk {
            gusb3.modify(GUSB3PIPECTL::U2SSINP3OK::Enable);
        }

        if self.req_p1p2p3_quirk {
            gusb3.modify(GUSB3PIPECTL::REQP0P1P2P3::Yes);
        }

        if self.del_p1p2p3_quirk {
            gusb3.modify(GUSB3PIPECTL::DEP1P2P3::Enable);
        }

        if self.del_phy_power_chg_quirk {
            gusb3.modify(GUSB3PIPECTL::DEPOCHANGE::Enable);
        }

        if self.lfps_filter_quirk {
            gusb3.modify(GUSB3PIPECTL::LFPSFILT::Enable);
        }

        if self.rx_detect_poll_quirk {
            gusb3.modify(GUSB3PIPECTL::RX_DETOPOLL::Enable);
        }

        if self.tx_de_emphasis_quirk {
            gusb3.modify(GUSB3PIPECTL::TX_DEEPH.val(self.tx_de_emphasis as u32));
        }

        /*
         * For some Rockchip SoCs like RK3588, if the USB3 PHY is suspended
         * in U-Boot would cause the PHY initialize abortively in Linux Kernel,
         * so disable the DWC3_GUSB3PIPECTL_SUSPHY feature here to fix it.
         */
        if self.dis_u3_susphy_quirk {
            gusb3.modify(GUSB3PIPECTL::SUSPHY::Disable);
        }

        self.dwc_regs.globals().gusb3pipectl0.set(gusb3.get());

        // 配置 USB2 High-Speed PHY 接口模式
        self.hsphy_mode_setup();

        crate::osal::kernel::delay(core::time::Duration::from_millis(100));

        // === USB2 PHY 配置 ===
        let mut gusb2 = self.dwc_regs.globals().gusb2phycfg0.extract();

        /*
         * Above 1.94a, it is recommended to set DWC3_GUSB2PHYCFG_SUSPHY to
         * '0' during coreConsultant configuration. So default value will
         * be '0' when the core is reset. Application needs to set it to
         * '1' after the core initialization is completed.
         */
        if self.revistion > DWC3_REVISION_194A {
            gusb2.modify(GUSB2PHYCFG::SUSPHY::Enable);
        }

        if self.dis_u2_susphy_quirk {
            gusb2.modify(GUSB2PHYCFG::SUSPHY::Disable);
        }

        if self.dis_enblslpm_quirk {
            gusb2.modify(GUSB2PHYCFG::ENBLSLPM::Disable);
        }

        if self.dis_u2_freeclk_exists_quirk {
            gusb2.modify(GUSB2PHYCFG::U2_FREECLK_EXISTS::No);
        }

        if self.usb2_phyif_utmi_width == 16 {
            // 清除 PHYIF 和 USBTRDTIM 字段
            gusb2.modify(
                GUSB2PHYCFG::PHYIF.val(1) + // UTMI_PHYIF_16_BIT
                GUSB2PHYCFG::USBTRDTIM.val(9), // USBTRDTIM_UTMI_16_BIT
            );
        }

        self.dwc_regs.globals().gusb2phycfg0.set(gusb2.get());

        crate::osal::kernel::delay(core::time::Duration::from_millis(100));

        debug!("DWC3: PHY configuration completed");

        Ok(())
    }

    fn alloc_scratch_buffers(&mut self) -> Result<()> {
        if !self.has_hibernation {
            return Ok(());
        }

        if self.nr_scratch == 0 {
            return Ok(());
        }

        let scratch_size = (self.nr_scratch as usize) * DWC3_SCRATCHBUF_SIZE;
        let scratchbuf = DVec::zeros(
            self.xhci.dma_mask as _,
            scratch_size,
            page_size(),
            dma_api::Direction::Bidirectional,
        )
        .map_err(|_| USBError::NoMemory)?;

        self.scratchbuf = Some(scratchbuf);
        debug!(
            "DWC3: Allocated {} scratch buffers (total {} bytes)",
            self.nr_scratch, scratch_size
        );

        Ok(())
    }

    fn setup_scratch_buffers(&mut self) {
        if let Some(_scratchbuf) = &self.scratchbuf {
            todo!()
        }
    }

    fn core_init_mode(&mut self) -> Result<()> {
        match self.dr_mode {
            DrMode::Host => {
                info!("DWC3: Initializing in HOST mode");
                self.dwc_regs.globals().gctl.modify(GCTL::PRTCAPDIR::Host);
            }
            DrMode::Otg => {
                todo!()
            }
            DrMode::Peripheral => todo!(),
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
